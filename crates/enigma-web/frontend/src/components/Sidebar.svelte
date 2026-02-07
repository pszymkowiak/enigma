<script>
  import { currentPage } from '../lib/stores.js';
  import { LayoutDashboard, HardDrive, FolderOpen, Network, Shield } from 'lucide-svelte';

  const nav = [
    { id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
    { id: 'storage', label: 'Storage', icon: HardDrive },
    { id: 'buckets', label: 'Buckets', icon: FolderOpen },
    { id: 'cluster', label: 'Cluster', icon: Network },
  ];
</script>

<aside class="w-60 bg-slate-900 border-r border-slate-800 flex flex-col">
  <div class="p-5 border-b border-slate-800">
    <div class="flex items-center gap-2">
      <Shield class="w-7 h-7 text-cyan-400" />
      <span class="text-xl font-bold text-white">Enigma</span>
    </div>
    <p class="text-xs text-slate-500 mt-1">Encrypted Storage</p>
  </div>

  <nav class="flex-1 py-4">
    {#each nav as item}
      <button
        class="w-full flex items-center gap-3 px-5 py-2.5 text-sm transition-colors {$currentPage === item.id
          ? 'bg-slate-800 text-cyan-400 border-r-2 border-cyan-400'
          : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'}"
        onclick={() => currentPage.set(item.id)}
      >
        <item.icon class="w-4 h-4" />
        {item.label}
      </button>
    {/each}
  </nav>

  <div class="p-4 border-t border-slate-800">
    <p class="text-xs text-slate-600">v0.1.0</p>
  </div>
</aside>
