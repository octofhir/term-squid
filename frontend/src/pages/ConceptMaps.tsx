import { createResource, createSignal, For, Show } from 'solid-js'
import { FiSearch, FiChevronLeft, FiChevronRight, FiChevronDown, FiChevronUp } from 'solid-icons/fi'
import styles from './ResourceList.module.css'

const PAGE_SIZE = 25

async function fetchConceptMaps([search, page, fhirVersion]: [string, number, string]) {
  const params = new URLSearchParams()
  if (search) params.set('name', search)
  params.set('_count', PAGE_SIZE.toString())
  params.set('_offset', ((page - 1) * PAGE_SIZE).toString())

  const response = await fetch(`/${fhirVersion}/ConceptMap?${params}`)
  return response.json()
}

export default function ConceptMaps() {
  const [search, setSearch] = createSignal('')
  const [debouncedSearch, setDebouncedSearch] = createSignal('')
  const [page, setPage] = createSignal(1)
  const [fhirVersion, setFhirVersion] = createSignal('r4')

  let debounceTimer: number

  const handleSearchInput = (value: string) => {
    setSearch(value)
    clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
      setDebouncedSearch(value)
      setPage(1)
    }, 300) as unknown as number
  }

  const [data] = createResource(
    () => [debouncedSearch(), page(), fhirVersion()] as [string, number, string],
    fetchConceptMaps
  )

  return (
    <div class={styles.container}>
      <header class={styles.header}>
        <div>
          <h1>ConceptMaps</h1>
          <p>Browse and search FHIR ConceptMaps</p>
        </div>

        <div class={styles.headerControls}>
          <select
            class={styles.filterSelect}
            value={fhirVersion()}
            onChange={(e) => {
              setFhirVersion(e.currentTarget.value)
              setPage(1)
            }}
          >
            <option value="r4">R4</option>
            <option value="r5">R5</option>
            <option value="r6">R6</option>
          </select>

          <div class={styles.searchBar}>
            <FiSearch class={styles.searchIcon} />
            <input
              type="search"
              placeholder="Search by name..."
              class={styles.searchInput}
              value={search()}
              onInput={(e) => handleSearchInput(e.currentTarget.value)}
            />
          </div>
        </div>
      </header>

      <Show
        when={data()}
        fallback={
          <div class={styles.loading}>
            <div class={styles.spinner} />
            <p>Loading ConceptMaps...</p>
          </div>
        }
      >
        <Show
          when={data()?.entry?.length > 0}
          fallback={
            <div class={styles.empty}>
              <p>No ConceptMaps found</p>
              <p class={styles.emptyHint}>Try adjusting your search criteria</p>
            </div>
          }
        >
          <div class={styles.contentWrapper}>
            <div class={styles.pagination} style={{ opacity: data.loading ? '0.6' : '1' }}>
              <button
                class={styles.pageButton}
                disabled={page() === 1}
                onClick={() => setPage(p => p - 1)}
              >
                <FiChevronLeft /> Previous
              </button>
              <span class={styles.pageInfo}>
                Page {page()} of {Math.ceil((data()?.total || 0) / PAGE_SIZE)} • {data()?.total || 0} total
              </span>
              <button
                class={styles.pageButton}
                disabled={page() >= Math.ceil((data()?.total || 0) / PAGE_SIZE)}
                onClick={() => setPage(p => p + 1)}
              >
                Next <FiChevronRight />
              </button>
            </div>

            <div class={styles.scrollableContent}>
              <div class={styles.list} style={{ opacity: data.loading ? '0.6' : '1', 'pointer-events': data.loading ? 'none' : 'auto' }}>
            <For each={data()?.entry}>
              {(item: any) => {
                const [expanded, setExpanded] = createSignal(false)
                const [showFullDesc, setShowFullDesc] = createSignal(false)
                const mappings = () => {
                  return item.resource.group?.flatMap((g: any) =>
                    g.element?.map((e: any) => ({
                      source: e.code,
                      targets: e.target?.map((t: any) => t.code).join(', ') || 'unmapped'
                    })) || []
                  ).slice(0, 50) || []
                }
                const hasLongDescription = () => item.resource.description?.length > 120

                return (
                  <div class={styles.item}>
                    <div class={styles.itemHeader}>
                      <h3>{item.resource.title || item.resource.name || 'Untitled'}</h3>
                      <div class={styles.meta}>
                        <span class={styles.status} data-status={item.resource.status}>
                          {item.resource.status}
                        </span>
                        <Show when={item.resource.version}>
                          <span class={styles.version}>v{item.resource.version}</span>
                        </Show>
                      </div>
                    </div>

                    <p class={styles.url}>{item.resource.url}</p>

                    <Show when={item.resource.description}>
                      <div>
                        <p class={showFullDesc() ? styles.descriptionFull : styles.description}>
                          {item.resource.description}
                        </p>
                        <Show when={hasLongDescription()}>
                          <button
                            class={styles.showMoreButton}
                            onClick={() => setShowFullDesc(!showFullDesc())}
                          >
                            {showFullDesc() ? 'Show less' : 'Show more'}
                          </button>
                        </Show>
                      </div>
                    </Show>

                    <div class={styles.itemFooter}>
                      <Show when={item.resource.publisher}>
                        <span class={styles.publisher}>
                          Publisher: {item.resource.publisher}
                        </span>
                      </Show>
                      <Show when={item.resource.group?.length}>
                        <span class={styles.content}>
                          Groups: {item.resource.group.length}
                        </span>
                      </Show>
                    </div>

                    <Show when={mappings().length > 0}>
                      <button
                        class={styles.expandButton}
                        onClick={() => setExpanded(!expanded())}
                      >
                        {expanded() ? <FiChevronUp /> : <FiChevronDown />}
                        {expanded() ? 'Hide' : 'Show'} Mappings ({mappings().length})
                      </button>

                      <Show when={expanded()}>
                        <div class={styles.expandedContent}>
                          <div class={styles.conceptList}>
                            <For each={mappings()}>
                              {(mapping: any) => (
                                <div class={styles.conceptItem}>
                                  <span class={styles.conceptCode}>{mapping.source}</span>
                                  <span class={styles.conceptDisplay}>→ {mapping.targets}</span>
                                </div>
                              )}
                            </For>
                          </div>
                        </div>
                      </Show>
                    </Show>
                  </div>
                )
              }}
            </For>
              </div>
            </div>
          </div>
        </Show>
      </Show>
    </div>
  )
}