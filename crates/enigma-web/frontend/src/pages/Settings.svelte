<script>
  import { onMount } from 'svelte';
  import { currentUser } from '../lib/stores.js';
  import { updatePassword, getTokens, createToken, revokeToken, updateUser, getMe } from '../lib/api.js';
  import Modal from '../components/Modal.svelte';
  import TokenReveal from '../components/TokenReveal.svelte';
  import { Plus, Trash2, Key } from 'lucide-svelte';

  let user = $derived($currentUser);
  let tokens = $state([]);
  let loading = $state(true);
  let error = $state('');
  let success = $state('');

  // Profile
  let email = $state('');
  let newPassword = $state('');
  let profileLoading = $state(false);

  // Token creation
  let showCreateToken = $state(false);
  let tokenName = $state('');
  let tokenScopes = $state('*');
  let tokenExpiry = $state('');
  let createdToken = $state('');
  let tokenError = $state('');

  onMount(async () => {
    email = user?.email || '';
    await loadTokens();
  });

  async function loadTokens() {
    loading = true;
    try {
      tokens = await getTokens();
    } catch (e) {
      error = e.message;
    }
    loading = false;
  }

  async function handleProfile(e) {
    e.preventDefault();
    profileLoading = true;
    error = ''; success = '';
    try {
      if (email !== (user?.email || '')) {
        await updateUser(user.id, { email });
      }
      if (newPassword) {
        await updatePassword(user.id, newPassword);
        newPassword = '';
      }
      const me = await getMe();
      currentUser.set(me);
      success = 'Profile updated';
    } catch (e) {
      error = e.message;
    }
    profileLoading = false;
  }

  async function handleCreateToken(e) {
    e.preventDefault();
    tokenError = '';
    createdToken = '';
    try {
      const result = await createToken(
        tokenName,
        tokenScopes,
        tokenExpiry ? parseInt(tokenExpiry) : null
      );
      createdToken = result.raw_token;
      tokenName = ''; tokenScopes = '*'; tokenExpiry = '';
      await loadTokens();
    } catch (e) {
      tokenError = e.message;
    }
  }

  async function handleRevoke(id) {
    if (!confirm('Revoke this API token?')) return;
    try {
      await revokeToken(id);
      await loadTokens();
    } catch (e) {
      error = e.message;
    }
  }
</script>

<div class="space-y-8 max-w-3xl">
  <!-- Profile -->
  <div class="bg-slate-900 border border-slate-800 rounded-xl p-6">
    <h2 class="text-lg font-semibold text-white mb-4">Profile</h2>

    {#if error}
      <div class="bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-lg px-4 py-2 mb-4">{error}</div>
    {/if}
    {#if success}
      <div class="bg-green-500/10 border border-green-500/20 text-green-400 text-sm rounded-lg px-4 py-2 mb-4">{success}</div>
    {/if}

    <form onsubmit={handleProfile} class="space-y-4">
      <div>
        <label class="block text-sm text-slate-400 mb-1">Username</label>
        <input value={user?.username || ''} disabled class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-500 text-sm" />
      </div>
      <div>
        <label class="block text-sm text-slate-400 mb-1">Email</label>
        <input bind:value={email} type="email" class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" />
      </div>
      <div>
        <label class="block text-sm text-slate-400 mb-1">New Password (leave empty to keep)</label>
        <input bind:value={newPassword} type="password" minlength="4" class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" />
      </div>
      <button
        type="submit"
        disabled={profileLoading}
        class="bg-cyan-600 hover:bg-cyan-500 disabled:bg-slate-700 text-white font-medium px-6 py-2 rounded-lg text-sm transition-colors"
      >
        {profileLoading ? 'Saving...' : 'Save Changes'}
      </button>
    </form>
  </div>

  <!-- API Tokens -->
  <div class="bg-slate-900 border border-slate-800 rounded-xl p-6">
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-lg font-semibold text-white">API Tokens</h2>
      <button
        onclick={() => { showCreateToken = true; createdToken = ''; }}
        class="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 text-white text-sm px-4 py-2 rounded-lg transition-colors"
      >
        <Plus class="w-4 h-4" /> New Token
      </button>
    </div>

    {#if loading}
      <p class="text-slate-400 text-sm">Loading...</p>
    {:else if tokens.length === 0}
      <p class="text-slate-500 text-sm">No API tokens yet.</p>
    {:else}
      <div class="space-y-2">
        {#each tokens as tok}
          <div class="flex items-center justify-between bg-slate-800 rounded-lg px-4 py-3">
            <div>
              <div class="flex items-center gap-2">
                <Key class="w-4 h-4 text-slate-500" />
                <span class="text-sm text-white font-medium">{tok.name}</span>
                <code class="text-xs text-slate-500">{tok.token_prefix}...</code>
              </div>
              <div class="text-xs text-slate-500 mt-1">
                Created {tok.created_at}
                {#if tok.last_used_at}
                  &middot; Last used {tok.last_used_at}
                {/if}
                {#if tok.expires_at}
                  &middot; Expires {tok.expires_at}
                {/if}
              </div>
            </div>
            <button
              onclick={() => handleRevoke(tok.id)}
              class="p-1.5 rounded hover:bg-slate-700 text-slate-400 hover:text-red-400 transition-colors"
              title="Revoke token"
            >
              <Trash2 class="w-4 h-4" />
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<!-- Create Token Modal -->
<Modal title="New API Token" show={showCreateToken} onclose={() => showCreateToken = false}>
  {#snippet children()}
    {#if createdToken}
      <TokenReveal token={createdToken} />
    {:else}
      <form onsubmit={handleCreateToken} class="space-y-4">
        {#if tokenError}
          <div class="bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-lg px-4 py-2">{tokenError}</div>
        {/if}
        <div>
          <label class="block text-sm text-slate-400 mb-1">Token Name</label>
          <input bind:value={tokenName} required class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" placeholder="e.g. CI Pipeline" />
        </div>
        <div>
          <label class="block text-sm text-slate-400 mb-1">Scopes</label>
          <input bind:value={tokenScopes} class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" placeholder="* (all)" />
        </div>
        <div>
          <label class="block text-sm text-slate-400 mb-1">Expires in (days, empty = never)</label>
          <input bind:value={tokenExpiry} type="number" min="1" class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" />
        </div>
        <button type="submit" class="w-full bg-cyan-600 hover:bg-cyan-500 text-white font-medium py-2 rounded-lg text-sm transition-colors">
          Create Token
        </button>
      </form>
    {/if}
  {/snippet}
</Modal>
