<script>
  let { title, value, subtitle = '', icon: Icon = null, color = 'cyan' } = $props();

  const colors = {
    cyan: 'text-cyan-400 bg-cyan-400/10',
    blue: 'text-blue-400 bg-blue-400/10',
    green: 'text-emerald-400 bg-emerald-400/10',
    amber: 'text-amber-400 bg-amber-400/10',
    purple: 'text-purple-400 bg-purple-400/10',
  };

  let colorClass = $derived(colors[color] || colors.cyan);
</script>

<div class="bg-slate-900 rounded-lg border border-slate-800 p-5">
  <div class="flex items-start justify-between">
    <div>
      <p class="text-sm text-slate-400">{title}</p>
      <p class="text-2xl font-bold text-white mt-1">{value}</p>
      {#if subtitle}
        <p class="text-xs text-slate-500 mt-1">{subtitle}</p>
      {/if}
    </div>
    {#if Icon}
      <div class="p-2 rounded-lg {colorClass}">
        <Icon class="w-5 h-5" />
      </div>
    {/if}
  </div>
</div>
