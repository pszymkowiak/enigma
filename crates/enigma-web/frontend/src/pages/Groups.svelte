<script>
  import { onMount } from 'svelte';
  import { getGroups, createGroup, deleteGroup, getPermissions, addGroupPermission, removeGroupPermission } from '../lib/api.js';
  import Modal from '../components/Modal.svelte';
  import { Plus, Trash2, Key } from 'lucide-svelte';

  let groups = $state([]);
  let allPermissions = $state([]);
  let loading = $state(true);
  let error = $state('');

  // Create group modal
  let showCreate = $state(false);
  let newName = $state('');
  let newDescription = $state('');
  let createError = $state('');

  // Permissions modal
  let showPerms = $state(false);
  let selectedGroup = $state(null);

  onMount(loadData);

  async function loadData() {
    loading = true;
    try {
      [groups, allPermissions] = await Promise.all([getGroups(), getPermissions()]);
    } catch (e) {
      error = e.message;
    }
    loading = false;
  }

  async function handleCreate(e) {
    e.preventDefault();
    createError = '';
    try {
      await createGroup(newName, newDescription);
      showCreate = false;
      newName = ''; newDescription = '';
      await loadData();
    } catch (e) {
      createError = e.message;
    }
  }

  async function handleDelete(group) {
    if (group.is_system) return;
    if (!confirm(`Delete group "${group.name}"?`)) return;
    try {
      await deleteGroup(group.id);
      await loadData();
    } catch (e) {
      error = e.message;
    }
  }

  function openPermissions(group) {
    selectedGroup = group;
    showPerms = true;
  }

  async function togglePermission(permId) {
    const hasPerm = selectedGroup.permissions.some(p => p.id === permId);
    try {
      if (hasPerm) {
        await removeGroupPermission(selectedGroup.id, permId);
      } else {
        await addGroupPermission(selectedGroup.id, permId);
      }
      await loadData();
      selectedGroup = groups.find(g => g.id === selectedGroup.id);
    } catch (e) {
      error = e.message;
    }
  }
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <h2 class="text-xl font-semibold text-white">Groups</h2>
    <button
      onclick={() => showCreate = true}
      class="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 text-white text-sm px-4 py-2 rounded-lg transition-colors"
    >
      <Plus class="w-4 h-4" /> New Group
    </button>
  </div>

  {#if error}
    <div class="bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-lg px-4 py-2.5">{error}</div>
  {/if}

  {#if loading}
    <p class="text-slate-400">Loading...</p>
  {:else}
    <div class="grid gap-4">
      {#each groups as group}
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-5">
          <div class="flex items-start justify-between">
            <div>
              <div class="flex items-center gap-2">
                <h3 class="text-white font-medium">{group.name}</h3>
                {#if group.is_system}
                  <span class="px-2 py-0.5 rounded-full text-xs bg-amber-500/10 text-amber-400 border border-amber-500/20">System</span>
                {/if}
              </div>
              <p class="text-sm text-slate-400 mt-1">{group.description}</p>
            </div>
            <div class="flex items-center gap-1">
              <button
                onclick={() => openPermissions(group)}
                class="p-1.5 rounded hover:bg-slate-800 text-slate-400 hover:text-cyan-400 transition-colors"
                title="Manage permissions"
              >
                <Key class="w-4 h-4" />
              </button>
              {#if !group.is_system}
                <button
                  onclick={() => handleDelete(group)}
                  class="p-1.5 rounded hover:bg-slate-800 text-slate-400 hover:text-red-400 transition-colors"
                  title="Delete group"
                >
                  <Trash2 class="w-4 h-4" />
                </button>
              {/if}
            </div>
          </div>
          <div class="flex flex-wrap gap-1.5 mt-3">
            {#each group.permissions as perm}
              <span class="px-2 py-0.5 rounded text-xs bg-slate-800 text-slate-300 border border-slate-700">
                {perm.action}
              </span>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- Create Group Modal -->
<Modal title="New Group" show={showCreate} onclose={() => showCreate = false}>
  {#snippet children()}
    <form onsubmit={handleCreate} class="space-y-4">
      {#if createError}
        <div class="bg-red-500/10 border border-red-500/20 text-red-400 text-sm rounded-lg px-4 py-2">{createError}</div>
      {/if}
      <div>
        <label class="block text-sm text-slate-400 mb-1">Name</label>
        <input bind:value={newName} required class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" />
      </div>
      <div>
        <label class="block text-sm text-slate-400 mb-1">Description</label>
        <input bind:value={newDescription} class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-white text-sm focus:outline-none focus:border-cyan-500" />
      </div>
      <button type="submit" class="w-full bg-cyan-600 hover:bg-cyan-500 text-white font-medium py-2 rounded-lg text-sm transition-colors">
        Create Group
      </button>
    </form>
  {/snippet}
</Modal>

<!-- Permissions Modal -->
<Modal title={selectedGroup ? `Permissions: ${selectedGroup.name}` : 'Permissions'} show={showPerms} onclose={() => showPerms = false}>
  {#snippet children()}
    {#if selectedGroup}
      <div class="space-y-1.5 max-h-96 overflow-y-auto">
        {#each allPermissions as perm}
          {@const hasPerm = selectedGroup.permissions.some(p => p.id === perm.id)}
          <label class="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-slate-800 cursor-pointer transition-colors">
            <input
              type="checkbox"
              checked={hasPerm}
              onchange={() => togglePermission(perm.id)}
              class="rounded border-slate-600 bg-slate-800 text-cyan-500 focus:ring-cyan-500/30"
            />
            <div>
              <div class="text-sm text-white">{perm.action}</div>
              <div class="text-xs text-slate-500">{perm.description}</div>
            </div>
          </label>
        {/each}
      </div>
    {/if}
  {/snippet}
</Modal>
