<script>
  import { Copy, Check } from 'lucide-svelte';

  let { token = '' } = $props();
  let copied = $state(false);

  async function copyToken() {
    await navigator.clipboard.writeText(token);
    copied = true;
    setTimeout(() => copied = false, 2000);
  }
</script>

<div class="bg-slate-800 border border-amber-500/30 rounded-lg p-4">
  <p class="text-amber-400 text-xs font-medium mb-2">This token will only be shown once. Copy it now.</p>
  <div class="flex items-center gap-2">
    <code class="flex-1 text-sm text-green-400 bg-slate-950 rounded px-3 py-2 font-mono break-all select-all">
      {token}
    </code>
    <button
      onclick={copyToken}
      class="shrink-0 p-2 rounded-lg bg-slate-700 hover:bg-slate-600 text-slate-300 transition-colors"
      title="Copy token"
    >
      {#if copied}
        <Check class="w-4 h-4 text-green-400" />
      {:else}
        <Copy class="w-4 h-4" />
      {/if}
    </button>
  </div>
</div>
