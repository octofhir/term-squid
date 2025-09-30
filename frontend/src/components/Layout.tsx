import { A } from '@solidjs/router'
import { Show, createSignal } from 'solid-js'
import { FiDatabase, FiList, FiMap, FiMenu } from 'solid-icons/fi'
import styles from './Layout.module.css'

export default function Layout(props: any) {
  const [sidebarOpen, setSidebarOpen] = createSignal(true)

  return (
    <div class={styles.app}>
      <aside class={styles.sidebar} classList={{ [styles.collapsed]: !sidebarOpen() }}>
        <div class={styles.sidebarHeader}>
          <h1 class={styles.logo}>
            <img src="/logo-transparent.png" alt="Term Squid Logo" class={styles.logoImage} />
            <Show when={sidebarOpen()}>
              <span>Term Squid</span>
            </Show>
          </h1>
        </div>

        <nav class={styles.nav}>
          <A href="/" class={styles.navItem} activeClass={styles.active} end>
            <FiDatabase />
            <Show when={sidebarOpen()}>
              <span>Dashboard</span>
            </Show>
          </A>

          <A href="/codesystems" class={styles.navItem} activeClass={styles.active}>
            <FiList />
            <Show when={sidebarOpen()}>
              <span>CodeSystems</span>
            </Show>
          </A>

          <A href="/valuesets" class={styles.navItem} activeClass={styles.active}>
            <FiList />
            <Show when={sidebarOpen()}>
              <span>ValueSets</span>
            </Show>
          </A>

          <A href="/conceptmaps" class={styles.navItem} activeClass={styles.active}>
            <FiMap />
            <Show when={sidebarOpen()}>
              <span>ConceptMaps</span>
            </Show>
          </A>
        </nav>

        <button
          class={styles.toggleButton}
          onClick={() => setSidebarOpen(!sidebarOpen())}
        >
          <FiMenu />
        </button>
      </aside>

      <main class={styles.main}>
        {props.children}
      </main>
    </div>
  )
}