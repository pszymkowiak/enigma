<script>
  import { getProviders, getChunkStats, getBackups } from '../lib/api.js';
  import { HardDrive, Database, Archive } from 'lucide-svelte';

  let providers = $state([]);
  let chunkStats = $state(null);
  let backups = $state([]);
  let loading = $state(true);

  $effect(() => {
    Promise.all([getProviders(), getChunkStats(), getBackups()])
      .then(([p, c, b]) => {
        providers = p;
        chunkStats = c;
        backups = b;
        loading = false;
      })
      .catch(() => { loading = false; });
  });

  function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }
</script>

<div class="space-y-6">
  {#if loading}
    <div class="flex items-center justify-center py-20">
      <div class="w-8 h-8 border-2 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
    </div>
  {:else}
    <!-- Providers -->
    <div>
      <h2 class="text-lg font-semibold text-white flex items-center gap-2 mb-4">
        <HardDrive class="w-5 h-5 text-blue-400" />
        Storage Providers
      </h2>
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {#each providers as p}
          <div class="bg-slate-900 rounded-lg border border-slate-800 p-4">
            <div class="flex items-center justify-between mb-3">
              <h3 class="font-medium text-white">{p.name}</h3>
              <span class="text-xs px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-400">{p.provider_type}</span>
            </div>
            <dl class="space-y-1.5 text-sm">
              <div class="flex justify-between">
                <dt class="text-slate-400">Bucket</dt>
                <dd class="text-slate-200 font-mono text-xs">{p.bucket}</dd>
              </div>
              {#if p.region}
                <div class="flex justify-between">
                  <dt class="text-slate-400">Region</dt>
                  <dd class="text-slate-200">{p.region}</dd>
                </div>
              {/if}
              <div class="flex justify-between">
                <dt class="text-slate-400">Weight</dt>
                <dd class="text-slate-200">{p.weight}</dd>
              </div>
            </dl>
          </div>
        {/each}
        {#if providers.length === 0}
          <p class="text-slate-500 text-sm col-span-full">No providers configured</p>
        {/if}
      </div>
    </div>

    <!-- Chunk Stats -->
    {#if chunkStats}
      <div>
        <h2 class="text-lg font-semibold text-white flex items-center gap-2 mb-4">
          <Database class="w-5 h-5 text-cyan-400" />
          Chunk Statistics
        </h2>
        <div class="bg-slate-900 rounded-lg border border-slate-800 p-4">
          <div class="grid grid-cols-2 gap-4">
            <div>
              <p class="text-sm text-slate-400">Total Chunks</p>
              <p class="text-xl font-bold text-white">{chunkStats.total_chunks.toLocaleString()}</p>
            </div>
            <div>
              <p class="text-sm text-slate-400">Orphan Chunks</p>
              <p class="text-xl font-bold {chunkStats.orphan_chunks > 0 ? 'text-amber-400' : 'text-emerald-400'}">
                {chunkStats.orphan_chunks.toLocaleString()}
              </p>
            </div>
          </div>
        </div>
      </div>
    {/if}

    <!-- Backups -->
    <div>
      <h2 class="text-lg font-semibold text-white flex items-center gap-2 mb-4">
        <Archive class="w-5 h-5 text-emerald-400" />
        Backups
      </h2>
      {#if backups.length > 0}
        <div class="bg-slate-900 rounded-lg border border-slate-800 overflow-hidden">
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-slate-800">
                <th class="text-left text-slate-400 font-medium px-4 py-3">ID</th>
                <th class="text-left text-slate-400 font-medium px-4 py-3">Source</th>
                <th class="text-left text-slate-400 font-medium px-4 py-3">Status</th>
                <th class="text-right text-slate-400 font-medium px-4 py-3">Files</th>
                <th class="text-right text-slate-400 font-medium px-4 py-3">Size</th>
                <th class="text-left text-slate-400 font-medium px-4 py-3">Created</th>
              </tr>
            </thead>
            <tbody>
              {#each backups as b}
                <tr class="border-b border-slate-800/50 hover:bg-slate-800/30">
                  <td class="px-4 py-2.5 text-slate-200 font-mono text-xs">{b.id.slice(0, 8)}...</td>
                  <td class="px-4 py-2.5 text-slate-200">{b.source_path}</td>
                  <td class="px-4 py-2.5">
                    <span class="text-xs px-2 py-0.5 rounded-full {
                      b.status === 'completed' ? 'bg-emerald-500/10 text-emerald-400' :
                      b.status === 'failed' ? 'bg-red-500/10 text-red-400' :
                      'bg-amber-500/10 text-amber-400'
                    }">{b.status}</span>
                  </td>
                  <td class="px-4 py-2.5 text-right text-slate-200">{b.total_files}</td>
                  <td class="px-4 py-2.5 text-right text-slate-200">{formatBytes(b.total_bytes)}</td>
                  <td class="px-4 py-2.5 text-slate-400 text-xs">{b.created_at}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="text-slate-500 text-sm">No backups yet</p>
      {/if}
    </div>
  {/if}
</div>
