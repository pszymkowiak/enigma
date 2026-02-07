<script>
  import { onMount } from 'svelte';
  import { getAuditLog } from '../lib/api.js';
  import { ChevronLeft, ChevronRight } from 'lucide-svelte';

  let entries = $state([]);
  let loading = $state(true);
  let error = $state('');
  let offset = $state(0);
  const limit = 50;

  onMount(() => loadPage(0));

  async function loadPage(newOffset) {
    loading = true;
    offset = newOffset;
    try {
      entries = await getAuditLog(limit, offset);
    } catch (e) {
      error = e.message;
    }
    loading = false;
  }
</script>

<div class="space-y-6">
  <h2 class="text-xl font-semibold text-white">Audit Log</h2>

  {#if error}
    <div class="bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-lg px-4 py-2.5">{error}</div>
  {/if}

  {#if loading}
    <p class="text-slate-400">Loading...</p>
  {:else}
    <div class="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-slate-800 text-slate-400">
            <th class="text-left px-4 py-3 font-medium">Timestamp</th>
            <th class="text-left px-4 py-3 font-medium">User</th>
            <th class="text-left px-4 py-3 font-medium">Action</th>
            <th class="text-left px-4 py-3 font-medium">Target</th>
            <th class="text-left px-4 py-3 font-medium">IP</th>
          </tr>
        </thead>
        <tbody>
          {#each entries as entry}
            <tr class="border-b border-slate-800/50 hover:bg-slate-800/30">
              <td class="px-4 py-3 text-slate-500 text-xs font-mono">{entry.created_at}</td>
              <td class="px-4 py-3 text-slate-300">{entry.user_id || '-'}</td>
              <td class="px-4 py-3">
                <span class="px-2 py-0.5 rounded text-xs bg-slate-800 text-cyan-400 border border-slate-700">
                  {entry.action}
                </span>
              </td>
              <td class="px-4 py-3 text-slate-400 text-xs">{entry.target || '-'}</td>
              <td class="px-4 py-3 text-slate-500 text-xs">{entry.ip_addr || '-'}</td>
            </tr>
          {/each}
          {#if entries.length === 0}
            <tr>
              <td colspan="5" class="px-4 py-8 text-center text-slate-500">No audit entries</td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>

    <div class="flex items-center justify-between">
      <button
        onclick={() => loadPage(Math.max(0, offset - limit))}
        disabled={offset === 0}
        class="flex items-center gap-1 text-sm text-slate-400 hover:text-white disabled:text-slate-600 transition-colors"
      >
        <ChevronLeft class="w-4 h-4" /> Previous
      </button>
      <span class="text-xs text-slate-500">Showing {offset + 1} - {offset + entries.length}</span>
      <button
        onclick={() => loadPage(offset + limit)}
        disabled={entries.length < limit}
        class="flex items-center gap-1 text-sm text-slate-400 hover:text-white disabled:text-slate-600 transition-colors"
      >
        Next <ChevronRight class="w-4 h-4" />
      </button>
    </div>
  {/if}
</div>
