import { createResource, Show } from 'solid-js'
import styles from './Home.module.css'

async function fetchStats() {
  const response = await fetch('/stats')
  return response.json()
}

export default function Home() {
  const [stats] = createResource(fetchStats)

  return (
    <div class={styles.container}>
      <header class={styles.header}>
        <h1>FHIR Terminology Server</h1>
        <p>Best-in-class terminology service with pluggable storage</p>
      </header>

      <div class={styles.stats}>
        <Show when={!stats.loading} fallback={<p>Loading...</p>}>
          <div class={styles.stat}>
            <div class={styles.statValue}>{stats()?.code_systems || 0}</div>
            <div class={styles.statLabel}>CodeSystems</div>
          </div>

          <div class={styles.stat}>
            <div class={styles.statValue}>{stats()?.value_sets || 0}</div>
            <div class={styles.statLabel}>ValueSets</div>
          </div>

          <div class={styles.stat}>
            <div class={styles.statValue}>{stats()?.concept_maps || 0}</div>
            <div class={styles.statLabel}>ConceptMaps</div>
          </div>
        </Show>
      </div>

      <div class={styles.info}>
        <div class={styles.card}>
          <h2>Multi-Version Support</h2>
          <p>
            All endpoints are available under version-specific base URLs:
            <code>/r4</code>, <code>/r5</code>, and <code>/r6</code>
          </p>
        </div>

        <div class={styles.card}>
          <h2>FHIR Operations</h2>
          <ul>
            <li>$lookup - Code lookup with properties</li>
            <li>$validate-code - Validate codes</li>
            <li>$subsumes - Test subsumption</li>
            <li>$expand - Expand ValueSets</li>
            <li>$translate - Translate between systems</li>
          </ul>
        </div>

        <div class={styles.card}>
          <h2>API Endpoints</h2>
          <p>
            Access the API at <code>http://localhost:8081/api</code>
          </p>
          <ul>
            <li>GET /r4/metadata - CapabilityStatement</li>
            <li>GET /r4/CodeSystem - Search CodeSystems</li>
            <li>POST /r4/CodeSystem - Create CodeSystem</li>
          </ul>
        </div>
      </div>
    </div>
  )
}