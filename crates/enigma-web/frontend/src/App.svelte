<script>
  import Sidebar from './components/Sidebar.svelte';
  import Header from './components/Header.svelte';
  import Login from './pages/Login.svelte';
  import Dashboard from './pages/Dashboard.svelte';
  import Storage from './pages/Storage.svelte';
  import Buckets from './pages/Buckets.svelte';
  import Cluster from './pages/Cluster.svelte';
  import { token, currentPage } from './lib/stores.js';

  let page = $derived($currentPage);
  let isLoggedIn = $derived(!!$token);
</script>

{#if !isLoggedIn}
  <Login />
{:else}
  <div class="flex h-screen">
    <Sidebar />
    <div class="flex-1 flex flex-col overflow-hidden">
      <Header />
      <main class="flex-1 overflow-y-auto p-6 bg-slate-950">
        {#if page === 'dashboard'}
          <Dashboard />
        {:else if page === 'storage'}
          <Storage />
        {:else if page === 'buckets'}
          <Buckets />
        {:else if page === 'cluster'}
          <Cluster />
        {:else}
          <Dashboard />
        {/if}
      </main>
    </div>
  </div>
{/if}
