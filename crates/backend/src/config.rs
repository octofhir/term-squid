use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database_url: String,
    #[serde(default = "default_max_connections")]
    pub database_max_connections: u32,
    pub server_host: String,
    pub server_port: u16,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let config = Self {
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgresql://termserver:dev_password@localhost:5433/termserver".to_string()
            }),
            database_max_connections: parse_max_connections(
                &std::env::var("DATABASE_MAX_CONNECTIONS")
                    .unwrap_or_else(|_| default_max_connections().to_string()),
            )?,
            server_host: std::env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8081".to_string())
                .parse()?,
        };

        Ok(config)
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}

fn default_max_connections() -> u32 {
    5
}

fn parse_max_connections(value: &str) -> anyhow::Result<u32> {
    let limit = value.parse()?;
    anyhow::ensure!(
        (1..=64).contains(&limit),
        "DATABASE_MAX_CONNECTIONS must be between 1 and 64"
    );
    Ok(limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pool_capacity_has_explicit_validated_bounds() {
        for value in ["1", "5", "32", "64"] {
            assert!(parse_max_connections(value).is_ok());
        }
        for value in ["", "0", "65", "-1", "invalid"] {
            assert!(parse_max_connections(value).is_err());
        }
        assert_eq!(default_max_connections(), 5);
    }
}
