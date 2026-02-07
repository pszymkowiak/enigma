<script>
  import { getNamespaces, getObjects } from '../lib/api.js';
  import { FolderOpen, File, ChevronRight } from 'lucide-svelte';

  let namespaces = $state([]);
  let selectedNs = $state(null);
  let objects = $state([]);
  let loading = $state(true);
  let loadingObjects = $state(false);

  $effect(() => {
    getNamespaces().then(data => {
      namespaces = data;
      loading = false;
    }).catch(() => { loading = false; });
  });

  async function selectNamespace(ns) {
    selectedNs = ns;
    loadingObjects = true;
    try {
      objects = await getObjects(ns.name);
    } catch {
      objects = [];
    }
    loadingObjects = false;
  }

  function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }
</script>

<div class="space-y-6">
  <h2 class="text-lg font-semibold text-white flex items-center gap-2">
    <FolderOpen class="w-5 h-5 text-purple-400" />
    S3 Buckets
  </h2>

  {#if loading}
    <div class="flex items-center justify-center py-20">
      <div class="w-8 h-8 border-2 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
    </div>
  {:else}
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- Namespace list -->
      <div class="space-y-2">
        {#each namespaces as ns}
          <button
            class="w-full text-left bg-slate-900 rounded-lg border p-3 transition-colors flex items-center justify-between
                   {selectedNs?.id === ns.id ? 'border-cyan-500 bg-cyan-500/5' : 'border-slate-800 hover:border-slate-700'}"
            onclick={() => selectNamespace(ns)}
          >
            <div>
              <p class="font-medium text-white text-sm">{ns.name}</p>
              <p class="text-xs text-slate-400">{ns.object_count} objects</p>
            </div>
            <ChevronRight class="w-4 h-4 text-slate-500" />
          </button>
        {/each}
        {#if namespaces.length === 0}
          <p class="text-slate-500 text-sm">No buckets created yet</p>
        {/if}
      </div>

      <!-- Object list -->
      <div class="lg:col-span-2">
        {#if selectedNs}
          <div class="bg-slate-900 rounded-lg border border-slate-800">
            <div class="px-4 py-3 border-b border-slate-800">
              <h3 class="text-sm font-medium text-white">{selectedNs.name}</h3>
              <p class="text-xs text-slate-400">Created: {selectedNs.created_at}</p>
            </div>

            {#if loadingObjects}
              <div class="flex items-center justify-center py-10">
                <div class="w-6 h-6 border-2 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
              </div>
            {:else if objects.length > 0}
              <table class="w-full text-sm">
                <thead>
                  <tr class="border-b border-slate-800">
                    <th class="text-left text-slate-400 font-medium px-4 py-2.5">Key</th>
                    <th class="text-right text-slate-400 font-medium px-4 py-2.5">Size</th>
                    <th class="text-left text-slate-400 font-medium px-4 py-2.5">ETag</th>
                  </tr>
                </thead>
                <tbody>
                  {#each objects as obj}
                    <tr class="border-b border-slate-800/50 hover:bg-slate-800/30">
                      <td class="px-4 py-2 text-slate-200 flex items-center gap-2">
                        <File class="w-3.5 h-3.5 text-slate-500 shrink-0" />
                        <span class="truncate">{obj.key}</span>
                      </td>
                      <td class="px-4 py-2 text-right text-slate-300">{formatBytes(obj.size)}</td>
                      <td class="px-4 py-2 text-slate-400 font-mono text-xs">{obj.etag.slice(0, 12)}...</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {:else}
              <p class="text-slate-500 text-sm px-4 py-6 text-center">No objects in this bucket</p>
            {/if}
          </div>
        {:else}
          <div class="flex items-center justify-center py-20 text-slate-500 text-sm">
            Select a bucket to view objects
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
