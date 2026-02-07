<script>
  import { getCluster } from '../lib/api.js';
  import { Network, Server, Radio } from 'lucide-svelte';

  let cluster = $state(null);
  let loading = $state(true);

  $effect(() => {
    getCluster().then(data => {
      cluster = data;
      loading = false;
    }).catch(() => { loading = false; });
  });
</script>

<div class="space-y-6">
  <h2 class="text-lg font-semibold text-white flex items-center gap-2">
    <Network class="w-5 h-5 text-cyan-400" />
    Cluster Status
  </h2>

  {#if loading}
    <div class="flex items-center justify-center py-20">
      <div class="w-8 h-8 border-2 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
    </div>
  {:else if cluster}
    <div class="bg-slate-900 rounded-lg border border-slate-800 p-5">
      <div class="flex items-center gap-3 mb-6">
        <div class="p-2.5 rounded-lg bg-cyan-400/10">
          <Radio class="w-5 h-5 text-cyan-400" />
        </div>
        <div>
          <p class="font-medium text-white">Mode: {cluster.mode}</p>
          {#if cluster.node_id}
            <p class="text-sm text-slate-400">Node ID: {cluster.node_id}</p>
          {/if}
        </div>
      </div>

      {#if cluster.peers.length > 0}
        <h3 class="text-sm font-medium text-slate-300 mb-3">Peers</h3>
        <div class="space-y-2">
          {#each cluster.peers as peer}
            <div class="flex items-center gap-3 bg-slate-800/50 rounded-lg px-4 py-2.5">
              <Server class="w-4 h-4 text-slate-400" />
              <div>
                <p class="text-sm text-white">Node {peer.id}</p>
                <p class="text-xs text-slate-400 font-mono">{peer.addr}</p>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="bg-slate-800/30 rounded-lg px-4 py-8 text-center">
          <Server class="w-8 h-8 text-slate-600 mx-auto mb-2" />
          <p class="text-slate-400 text-sm">Running as a single node</p>
          <p class="text-slate-500 text-xs mt-1">Configure Raft peers to enable cluster mode</p>
        </div>
      {/if}
    </div>
  {:else}
    <p class="text-slate-400">Unable to load cluster info.</p>
  {/if}
</div>
