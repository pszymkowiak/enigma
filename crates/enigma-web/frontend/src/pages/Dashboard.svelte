<script>
  import StatCard from '../components/StatCard.svelte';
  import { getStatus } from '../lib/api.js';
  import { Database, HardDrive, FolderOpen, Shield, Archive, Layers } from 'lucide-svelte';

  let status = $state(null);
  let loading = $state(true);

  $effect(() => {
    getStatus().then(data => {
      status = data;
      loading = false;
    }).catch(() => {
      loading = false;
    });
  });
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-xl font-semibold text-white">Overview</h2>
    <p class="text-sm text-slate-400 mt-1">System status and statistics</p>
  </div>

  {#if loading}
    <div class="flex items-center justify-center py-20">
      <div class="w-8 h-8 border-2 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
    </div>
  {:else if status}
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
      <StatCard title="Providers" value={status.total_providers} icon={HardDrive} color="blue" />
      <StatCard title="Chunks" value={status.total_chunks.toLocaleString()} icon={Database} color="cyan" />
      <StatCard title="Backups" value={status.total_backups} icon={Archive} color="green" />
      <StatCard title="Buckets" value={status.total_namespaces} icon={FolderOpen} color="purple" />
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
      <div class="bg-slate-900 rounded-lg border border-slate-800 p-5">
        <h3 class="text-sm font-medium text-slate-300 mb-4">Configuration</h3>
        <dl class="space-y-3">
          <div class="flex justify-between">
            <dt class="text-sm text-slate-400">Version</dt>
            <dd class="text-sm text-white">{status.version}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-sm text-slate-400">Key Provider</dt>
            <dd class="text-sm text-white flex items-center gap-1.5">
              <Shield class="w-3.5 h-3.5 text-cyan-400" />
              {status.key_provider}
            </dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-sm text-slate-400">Distribution</dt>
            <dd class="text-sm text-white flex items-center gap-1.5">
              <Layers class="w-3.5 h-3.5 text-blue-400" />
              {status.distribution}
            </dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-sm text-slate-400">Compression</dt>
            <dd class="text-sm {status.compression_enabled ? 'text-emerald-400' : 'text-slate-500'}">
              {status.compression_enabled ? 'Enabled' : 'Disabled'}
            </dd>
          </div>
        </dl>
      </div>

      <div class="bg-slate-900 rounded-lg border border-slate-800 p-5">
        <h3 class="text-sm font-medium text-slate-300 mb-4">Quick Stats</h3>
        <dl class="space-y-3">
          <div class="flex justify-between">
            <dt class="text-sm text-slate-400">Storage Providers</dt>
            <dd class="text-sm text-white">{status.total_providers}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-sm text-slate-400">Total Chunks</dt>
            <dd class="text-sm text-white">{status.total_chunks.toLocaleString()}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-sm text-slate-400">Total Backups</dt>
            <dd class="text-sm text-white">{status.total_backups}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="text-sm text-slate-400">S3 Buckets</dt>
            <dd class="text-sm text-white">{status.total_namespaces}</dd>
          </div>
        </dl>
      </div>
    </div>
  {:else}
    <p class="text-slate-400">Unable to load status.</p>
  {/if}
</div>
