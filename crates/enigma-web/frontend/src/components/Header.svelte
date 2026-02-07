<script>
  import { token, currentPage } from '../lib/stores.js';
  import { LogOut, User } from 'lucide-svelte';

  const titles = {
    dashboard: 'Dashboard',
    storage: 'Storage',
    buckets: 'Buckets',
    cluster: 'Cluster',
  };

  let title = $derived(titles[$currentPage] || 'Dashboard');

  function logout() {
    token.set('');
    currentPage.set('dashboard');
  }
</script>

<header class="h-14 bg-slate-900 border-b border-slate-800 flex items-center justify-between px-6">
  <h1 class="text-lg font-semibold text-white">{title}</h1>
  <div class="flex items-center gap-4">
    <div class="flex items-center gap-2 text-sm text-slate-400">
      <User class="w-4 h-4" />
      <span>admin</span>
    </div>
    <button
      class="flex items-center gap-1.5 text-sm text-slate-400 hover:text-red-400 transition-colors"
      onclick={logout}
    >
      <LogOut class="w-4 h-4" />
      Logout
    </button>
  </div>
</header>
