<script>
  import EChart from './EChart.svelte';
  let { option, metric } = $props();
  const formatted = $derived({ ...option, tooltip: { ...option.tooltip,
    valueFormatter: value => typeof value === 'number'
      ? metric === 'rps' ? `${value.toFixed(1)} requests/s` : `${value.toFixed(3)} ms`
      : value } });
</script>

<EChart option={formatted} height="260px" />
