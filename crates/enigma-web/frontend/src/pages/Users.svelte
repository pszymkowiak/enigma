<script>
  import { onMount } from 'svelte';
  import { getUsers, createUser, deleteUser, getGroups, addUserGroup, removeUserGroup } from '../lib/api.js';
  import Modal from '../components/Modal.svelte';
  import { UserPlus, Trash2, Shield } from 'lucide-svelte';

  let users = $state([]);
  let groups = $state([]);
  let loading = $state(true);
  let error = $state('');

  // Create user modal
  let showCreate = $state(false);
  let newUsername = $state('');
  let newPassword = $state('');
  let newEmail = $state('');
  let createError = $state('');

  // Group assignment modal
  let showGroups = $state(false);
  let selectedUser = $state(null);

  onMount(loadData);

  async function loadData() {
    loading = true;
    try {
      [users, groups] = await Promise.all([getUsers(), getGroups()]);
    } catch (e) {
      error = e.message;
    }
    loading = false;
  }

  async function handleCreate(e) {
    e.preventDefault();
    createError = '';
    try {
      await createUser(newUsername, newPassword, newEmail);
      showCreate = false;
      newUsername = ''; newPassword = ''; newEmail = '';
      await loadData();
    } catch (e) {
      createError = e.message;
    }
  }

  async function handleDelete(user) {
    if (!confirm(`Delete user "${user.username}"?`)) return;
    try {
      await deleteUser(user.id);
      await loadData();
    } catch (e) {
      error = e.message;
    }
  }

  function openGroups(user) {
    selectedUser = user;
    showGroups = true;
  }

  async function toggleGroup(groupId) {
    const hasGroup = selectedUser.groups.some(g => g.id === groupId);
    try {
      if (hasGroup) {
        await removeUserGroup(selectedUser.id, groupId);
      } else {
        await addUserGroup(selectedUser.id, groupId);
      }
      await loadData();
      selectedUser = users.find(u => u.id === selectedUser.id);
    } catch (e) {
      error = e.message;
    }
  }
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <h2 class="text-xl font-semibold text-white">Users</h2>
    <button
      onclick={() => showCreate = true}
      class="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 text-white text-sm px-4 py-2 rounded-lg transition-colors"
    >
      <UserPlus class="w-4 h-4" /> New User
    </button>
  </div>

  {#if error}
    <div class="bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-lg px-4 py-2.5">{error}</div>
  {/if}

  {#if loading}
    <p class="text-slate-400">Loading...</p>
  {:else}
    <div class="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-slate-800 text-slate-400">
            <th class="text-left px-4 py-3 font-medium">Username</th>
            <th class="text-left px-4 py-3 font-medium">Email</th>
            <th class="text-left px-4 py-3 font-medium">Groups</th>
            <th class="text-left px-4 py-3 font-medium">Status</th>
            <th class="text-left px-4 py-3 font-medium">Created</th>
            <th class="text-right px-4 py-3 font-medium">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each users as user}
            <tr class="border-b border-slate-800/50 hover:bg-slate-800/30">
              <td class="px-4 py-3 text-white font-medium">{user.username}</td>
              <td class="px-4 py-3 text-slate-400">{user.email || '-'}</td>
              <td class="px-4 py-3">
                <div class="flex flex-wrap gap-1">
                  {#each user.groups as group}
                    <span class="px-2 py-0.5 rounded-full text-xs bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
                      {group.name}
                    </span>
                  {/each}
                </div>
              </td>
              <td class="px-4 py-3">
                <span class="px-2 py-0.5 rounded-full text-xs {user.is_active ? 'bg-green-500/10 text-green-400' : 'bg-red-500/10 text-red-400'}">
                  {user.is_active ? 'Active' : 'Inactive'}
                </span>
              </td>
              <td class="px-4 py-3 text-slate-500 text-xs">{user.created_at}</td>
              <td class="px-4 py-3 text-right">
                <div class="flex items-center justify-end gap-1">
                  <button
                    onclick={() => openGroups(user)}
                    class="p-1.5 rounded hover:bg-slate-700 text-slate-400 hover:text-cyan-400 transition-colors"
                    title="Manage groups"
                  >
                    <Shield class="w-4 h-4" />
                  </button>
                  <button
                    onclick={() => handleDelete(user)}
                    class="p-1.5 rounded hover:bg-slate-700 text-slate-400 hover:text-red-400 transition-colors"
                    title="Delete user"
                  >
                    <Trash2 class="w-4 h-4" />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<!-- Create User Modal -->
<Modal title="New User" show={showCreate} onclose={() => showCreate = false}>
  {#snippet children()}
    <form onsubmit={handleCreate} class="space-y-4">
      {#if createError}
        <div class="bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-lg px-4 py-2">{createError}</div>
      {/if}
      <div>
        <label class="block text-sm text-slate-400 mb-1">Username</label>
        <input bind:value={newUsername} required class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" />
      </div>
      <div>
        <label class="block text-sm text-slate-400 mb-1">Password</label>
        <input bind:value={newPassword} type="password" required minlength="4" class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" />
      </div>
      <div>
        <label class="block text-sm text-slate-400 mb-1">Email (optional)</label>
        <input bind:value={newEmail} type="email" class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" />
      </div>
      <button type="submit" class="w-full bg-cyan-600 hover:bg-cyan-500 text-white font-medium py-2 rounded-lg text-sm transition-colors">
        Create User
      </button>
    </form>
  {/snippet}
</Modal>

<!-- Group Assignment Modal -->
<Modal title={selectedUser ? `Groups: ${selectedUser.username}` : 'Groups'} show={showGroups} onclose={() => showGroups = false}>
  {#snippet children()}
    {#if selectedUser}
      <div class="space-y-2">
        {#each groups as group}
          {@const hasGroup = selectedUser.groups.some(g => g.id === group.id)}
          <button
            onclick={() => toggleGroup(group.id)}
            class="w-full flex items-center justify-between px-4 py-3 rounded-lg border transition-colors
                   {hasGroup ? 'border-cyan-500/30 bg-cyan-500/10' : 'border-slate-700 bg-slate-800 hover:border-slate-600'}"
          >
            <div class="text-left">
              <div class="text-sm font-medium {hasGroup ? 'text-cyan-400' : 'text-white'}">{group.name}</div>
              <div class="text-xs text-slate-500">{group.description}</div>
            </div>
            <div class="text-xs {hasGroup ? 'text-cyan-400' : 'text-slate-600'}">
              {hasGroup ? 'Assigned' : 'Not assigned'}
            </div>
          </button>
        {/each}
      </div>
    {/if}
  {/snippet}
</Modal>
