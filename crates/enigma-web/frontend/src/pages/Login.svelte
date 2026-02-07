<script>
  import { login } from '../lib/api.js';
  import { Shield } from 'lucide-svelte';

  let username = $state('');
  let password = $state('');
  let error = $state('');
  let loading = $state(false);

  async function handleSubmit(e) {
    e.preventDefault();
    error = '';
    loading = true;
    try {
      await login(username, password);
    } catch (err) {
      error = 'Invalid credentials';
    } finally {
      loading = false;
    }
  }
</script>

<div class="min-h-screen bg-slate-950 flex items-center justify-center">
  <div class="w-full max-w-sm">
    <div class="text-center mb-8">
      <div class="flex items-center justify-center gap-2 mb-3">
        <Shield class="w-10 h-10 text-cyan-400" />
        <h1 class="text-3xl font-bold text-white">Enigma</h1>
      </div>
      <p class="text-slate-400 text-sm">Encrypted Multi-Cloud Storage</p>
    </div>

    <form onsubmit={handleSubmit} class="bg-slate-900 rounded-lg border border-slate-800 p-6 space-y-4">
      {#if error}
        <div class="bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-lg px-4 py-2.5">
          {error}
        </div>
      {/if}

      <div>
        <label for="username" class="block text-sm text-slate-400 mb-1.5">Username</label>
        <input
          id="username"
          type="text"
          bind:value={username}
          class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm
                 focus:outline-none focus:border-cyan-500 focus:ring-1 focus:ring-cyan-500/30"
          placeholder="admin"
          required
        />
      </div>

      <div>
        <label for="password" class="block text-sm text-slate-400 mb-1.5">Password</label>
        <input
          id="password"
          type="password"
          bind:value={password}
          class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm
                 focus:outline-none focus:border-cyan-500 focus:ring-1 focus:ring-cyan-500/30"
          placeholder="••••••••"
          required
        />
      </div>

      <button
        type="submit"
        disabled={loading}
        class="w-full bg-cyan-600 hover:bg-cyan-500 disabled:bg-slate-700 disabled:text-slate-500
               text-white font-medium py-2.5 rounded-lg text-sm transition-colors"
      >
        {loading ? 'Signing in...' : 'Sign in'}
      </button>
    </form>
  </div>
</div>
