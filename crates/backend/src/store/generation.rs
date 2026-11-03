//! Fresh observations shared only by requests detached before SQL dispatch.
use crate::error::AppError;
use sqlx::PgPool;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::sync::oneshot;

const MAX_PENDING: usize = 256;

#[derive(Clone, Copy)]
pub(super) enum Family {
    ValueSet,
    Map,
    Hierarchy,
    Concept,
}

#[derive(Default, sqlx::FromRow)]
pub(super) struct Observation {
    pub generation: i64,
    pub key_a: String,
    pub key_b: String,
}

#[derive(sqlx::FromRow)]
struct Batch {
    generation: i64,
    maps_tracked: bool,
    hierarchies_tracked: bool,
    keys: Vec<String>,
}

type Reply = oneshot::Sender<Result<Observation, Arc<sqlx::Error>>>;
struct Pending {
    family: Family,
    keys: Option<(String, String)>,
    reply: Reply,
}
#[derive(Default)]
struct State {
    running: bool,
    pending: VecDeque<Pending>,
}
struct Inner {
    pool: PgPool,
    state: Mutex<State>,
    #[cfg(test)]
    dispatch: Arc<tokio::sync::Mutex<()>>,
    #[cfg(test)]
    delivery: Arc<tokio::sync::Mutex<()>>,
    #[cfg(test)]
    started: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    completed: std::sync::atomic::AtomicUsize,
}

pub(super) struct GenerationReads(Arc<Inner>);
impl GenerationReads {
    pub(super) fn new(pool: PgPool) -> Self {
        Self(Arc::new(Inner {
            pool,
            state: Mutex::new(State::default()),
            #[cfg(test)]
            dispatch: Arc::default(),
            #[cfg(test)]
            delivery: Arc::default(),
            #[cfg(test)]
            started: Default::default(),
            #[cfg(test)]
            completed: Default::default(),
        }))
    }
    pub(super) async fn current(&self, family: Family) -> Result<i64, AppError> {
        Ok(self.read(family, None).await?.generation)
    }
    pub(super) async fn hierarchy(&self, a: &str, b: &str) -> Result<Observation, AppError> {
        if a.len() > 4096 || b.len() > 4096 {
            return Err(AppError::BadRequest(
                "Generation probe exceeds preparation limit".into(),
            ));
        }
        self.read(Family::Hierarchy, Some((a.into(), b.into())))
            .await
    }
    pub(super) async fn concept(&self, code: &str) -> Result<Observation, AppError> {
        if code.len() > 4096 {
            return Err(AppError::BadRequest(
                "Generation probe exceeds preparation limit".into(),
            ));
        }
        self.read(Family::Concept, Some((code.into(), String::new())))
            .await
    }
    async fn read(
        &self,
        family: Family,
        keys: Option<(String, String)>,
    ) -> Result<Observation, AppError> {
        let (reply, receive) = oneshot::channel();
        let start = {
            let mut state = self.0.state.lock().expect("generation queue poisoned");
            if state.pending.len() == MAX_PENDING {
                state.pending.retain(|pending| !pending.reply.is_closed());
            }
            if state.pending.len() == MAX_PENDING {
                return Err(AppError::Overloaded);
            }
            state.pending.push_back(Pending {
                family,
                keys,
                reply,
            });
            let start = !state.running;
            state.running = true;
            start
        };
        if start {
            let runner = Runner {
                inner: self.0.clone(),
                active: true,
            };
            // Dropping an HTTP future cannot cancel an active SQL observation.
            tokio::spawn(runner.run());
        }
        receive
            .await
            .map_err(|_| AppError::Internal(anyhow::anyhow!("Generation worker stopped")))?
            .map_err(AppError::Generation)
    }

    #[cfg(test)]
    pub(super) async fn pause(&self, delivery: bool) -> tokio::sync::OwnedMutexGuard<()> {
        if delivery {
            self.0.delivery.clone()
        } else {
            self.0.dispatch.clone()
        }
        .lock_owned()
        .await
    }
    #[cfg(test)]
    pub(super) fn stats(&self) -> (usize, usize, usize) {
        use std::sync::atomic::Ordering::SeqCst;
        (
            self.0.started.load(SeqCst),
            self.0.completed.load(SeqCst),
            self.0
                .state
                .lock()
                .unwrap()
                .pending
                .iter()
                .filter(|p| !p.reply.is_closed())
                .count(),
        )
    }
}

struct Runner {
    inner: Arc<Inner>,
    active: bool,
}
impl Runner {
    async fn run(mut self) {
        loop {
            {
                let mut state = self.inner.state.lock().expect("generation queue poisoned");
                state.pending.retain(|pending| !pending.reply.is_closed());
                if state.pending.is_empty() {
                    state.running = false;
                    self.active = false;
                    return;
                }
            }
            #[cfg(test)]
            {
                let _gate = self.inner.dispatch.lock().await;
            }
            // Collect requests while checkout waits. No request can join the detached batch
            // after this point, even while its SQL result is still in flight.
            let connection = self.inner.pool.acquire().await;
            let pending = {
                let mut state = self.inner.state.lock().expect("generation queue poisoned");
                state.pending.retain(|pending| !pending.reply.is_closed());
                if state.pending.is_empty() {
                    state.running = false;
                    self.active = false;
                    return;
                }
                std::mem::take(&mut state.pending)
            };
            let keys: Vec<&str> = pending
                .iter()
                .filter_map(|p| p.keys.as_ref())
                .flat_map(|(a, b)| [a.as_str(), b.as_str()])
                .collect();
            #[cfg(test)]
            self.inner
                .started
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let result = match connection {
                Ok(mut connection) => sqlx::query_as::<_, Batch>(
                    "SELECT generation,maps_tracked,hierarchies_tracked,ARRAY(SELECT lower(value) FROM unnest($1::text[]) WITH ORDINALITY AS inputs(value,ord) ORDER BY ord) AS keys FROM terminology_generation WHERE singleton")
                    .bind(&keys).fetch_one(&mut *connection).await,
                Err(error) => Err(error),
            }.and_then(|batch| {
                if batch.keys.len() == keys.len() { Ok(batch) }
                else { Err(sqlx::Error::Protocol("Generation comparison-key count mismatch".into())) }
            }).map_err(Arc::new);
            #[cfg(test)]
            self.inner
                .completed
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            #[cfg(test)]
            let _delivery = self.inner.delivery.lock().await;
            match result {
                Err(error) => {
                    for request in pending {
                        let _ = request.reply.send(Err(error.clone()));
                    }
                }
                Ok(batch) => {
                    let mut keys = batch.keys.into_iter();
                    for request in pending {
                        let (key_a, key_b) = if request.keys.is_some() {
                            (keys.next().unwrap(), keys.next().unwrap())
                        } else {
                            (String::new(), String::new())
                        };
                        let tracked = match request.family {
                            Family::ValueSet => true,
                            Family::Map => batch.maps_tracked,
                            Family::Hierarchy | Family::Concept => batch.hierarchies_tracked,
                        };
                        let result = if tracked {
                            Ok(Observation {
                                generation: batch.generation,
                                key_a,
                                key_b,
                            })
                        } else {
                            Err(Arc::new(sqlx::Error::RowNotFound))
                        };
                        let _ = request.reply.send(result);
                    }
                }
            }
        }
    }
}
impl Drop for Runner {
    fn drop(&mut self) {
        if self.active {
            // A stopped/panicked runtime must not leave a permanently running queue.
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.running = false;
            state.pending.clear();
        }
    }
}
