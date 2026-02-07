<script>
  import { currentPage, sidebarCollapsed } from '../lib/stores.js';
  import { LayoutDashboard, HardDrive, FolderOpen, Network, Shield, PanelLeftClose, PanelLeftOpen } from 'lucide-svelte';

  const nav = [
    { id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
    { id: 'storage', label: 'Storage', icon: HardDrive },
    { id: 'buckets', label: 'Buckets', icon: FolderOpen },
    { id: 'cluster', label: 'Cluster', icon: Network },
  ];

  let collapsed = $derived($sidebarCollapsed);

  function toggle() {
    sidebarCollapsed.update(v => !v);
  }
</script>

<aside class="transition-all duration-200 {collapsed ? 'w-16' : 'w-60'} bg-slate-900 border-r border-slate-800 flex flex-col shrink-0">
  <div class="p-3 border-b border-slate-800 {collapsed ? 'flex justify-center' : 'px-5 py-5'}">
    {#if collapsed}
      <Shield class="w-7 h-7 text-cyan-400" />
    {:else}
      <div class="flex items-center gap-2">
        <Shield class="w-7 h-7 text-cyan-400" />
        <span class="text-xl font-bold text-white">Enigma</span>
      </div>
      <p class="text-xs text-slate-500 mt-1">Encrypted Storage</p>
    {/if}
  </div>

  <nav class="flex-1 py-4">
    {#each nav as item}
      <button
        title={collapsed ? item.label : ''}
        class="w-full flex items-center transition-colors
               {collapsed ? 'justify-center px-0 py-3' : 'gap-3 px-5 py-2.5'}
               text-sm {$currentPage === item.id
                 ? 'bg-slate-800 text-cyan-400 border-r-2 border-cyan-400'
                 : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'}"
        onclick={() => currentPage.set(item.id)}
      >
        <item.icon class="w-4 h-4 shrink-0" />
        {#if !collapsed}
          {item.label}
        {/if}
      </button>
    {/each}
  </nav>

  <div class="border-t border-slate-800">
    <button
      class="w-full flex items-center text-slate-500 hover:text-slate-300 transition-colors
             {collapsed ? 'justify-center py-3' : 'gap-2 px-5 py-3'}"
      onclick={toggle}
      title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
    >
      {#if collapsed}
        <PanelLeftOpen class="w-4 h-4" />
      {:else}
        <PanelLeftClose class="w-4 h-4" />
        <span class="text-xs">Réduire</span>
      {/if}
    </button>
  </div>
</aside>
