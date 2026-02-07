<script>
  import { browseFiles, downloadFile, deleteFile, createFolder } from '../lib/api.js';
  import { enqueueUploads, isUploading as isUploadingStore, uploadQueue as uploadQueueStore } from '../lib/stores.js';
  import { FolderOpen, File, FileText, Image, Video, Archive, Upload, FolderPlus, Download, Trash2, ChevronRight, Home, X } from 'lucide-svelte';

  let currentPath = $state('');
  let folders = $state([]);
  let files = $state([]);
  let loading = $state(true);
  let error = $state('');

  let dragOver = $state(false);
  let dragDepth = $state(0);

  // Modal state
  let showNewFolder = $state(false);
  let newFolderName = $state('');
  let showDeleteConfirm = $state(false);
  let deleteTarget = $state(null);

  // Subscribe to upload completions to refresh file list
  let prevUploading = false;
  $effect(() => {
    const nowUploading = $isUploadingStore;
    if (prevUploading && !nowUploading && $uploadQueueStore.length > 0) {
      browse(currentPath);
    }
    prevUploading = nowUploading;
  });

  async function browse(path = '') {
    loading = true;
    error = '';
    try {
      const data = await browseFiles(path);
      currentPath = data.path;
      folders = data.folders;
      files = data.files;
    } catch (e) {
      error = e.message;
      folders = [];
      files = [];
    }
    loading = false;
  }

  // Initial load
  $effect(() => {
    browse('');
  });

  function navigateTo(path) {
    browse(path);
  }

  // Breadcrumb segments
  let breadcrumbs = $derived(() => {
    if (!currentPath) return [];
    const parts = currentPath.replace(/\/$/, '').split('/').filter(Boolean);
    let acc = '';
    return parts.map(p => {
      acc += p + '/';
      return { name: p, path: acc };
    });
  });

  // -- File type icons --
  function getFileIcon(name) {
    const ext = name.split('.').pop()?.toLowerCase();
    const images = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico', 'tiff'];
    const videos = ['mp4', 'avi', 'mov', 'mkv', 'webm', 'flv', 'wmv'];
    const docs = ['pdf', 'doc', 'docx', 'txt', 'md', 'rtf', 'odt', 'xls', 'xlsx', 'csv'];
    const archives = ['zip', 'tar', 'gz', 'rar', '7z', 'bz2', 'xz', 'zst'];

    if (images.includes(ext)) return 'image';
    if (videos.includes(ext)) return 'video';
    if (docs.includes(ext)) return 'doc';
    if (archives.includes(ext)) return 'archive';
    return 'file';
  }

  // -- Folder drag & drop with webkitGetAsEntry --
  async function getFilesFromDataTransfer(dataTransfer) {
    const items = [...dataTransfer.items];
    const fileList = [];

    for (const item of items) {
      const entry = item.webkitGetAsEntry?.();
      if (entry) {
        await traverseEntry(entry, '', fileList);
      } else if (item.kind === 'file') {
        const file = item.getAsFile();
        if (file) fileList.push({ file, relativePath: file.name });
      }
    }
    return fileList;
  }

  async function traverseEntry(entry, basePath, fileList) {
    if (entry.isFile) {
      const file = await new Promise((resolve, reject) => entry.file(resolve, reject));
      fileList.push({ file, relativePath: basePath + entry.name });
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      let entries = [];
      let batch;
      do {
        batch = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
        entries.push(...batch);
      } while (batch.length > 0);
      for (const child of entries) {
        await traverseEntry(child, basePath + entry.name + '/', fileList);
      }
    }
  }

  // -- Event handlers --
  async function handleUpload(fileList) {
    if (!fileList || fileList.length === 0) return;
    const entries = Array.from(fileList).map(f => ({ file: f, relativePath: f.name }));
    enqueueUploads(entries, currentPath);
  }

  function onFileInput(e) {
    handleUpload(e.target.files);
    e.target.value = '';
  }

  async function onDrop(e) {
    e.preventDefault();
    dragOver = false;
    dragDepth = 0;

    // Try webkitGetAsEntry for folder support
    if (e.dataTransfer.items && e.dataTransfer.items.length > 0) {
      const hasEntries = [...e.dataTransfer.items].some(item => item.webkitGetAsEntry?.());
      if (hasEntries) {
        const fileEntries = await getFilesFromDataTransfer(e.dataTransfer);
        if (fileEntries.length > 0) {
          enqueueUploads(fileEntries, currentPath);
          return;
        }
      }
    }

    // Fallback to plain files
    handleUpload(e.dataTransfer.files);
  }

  function onDragEnter(e) {
    e.preventDefault();
    dragDepth++;
    dragOver = true;
  }

  function onDragOver(e) {
    e.preventDefault();
  }

  function onDragLeave(e) {
    dragDepth--;
    if (dragDepth <= 0) {
      dragOver = false;
      dragDepth = 0;
    }
  }

  async function handleCreateFolder() {
    if (!newFolderName.trim()) return;
    error = '';
    try {
      const path = currentPath + newFolderName.trim();
      await createFolder(path);
      showNewFolder = false;
      newFolderName = '';
      await browse(currentPath);
    } catch (e) {
      error = e.message;
    }
  }

  function confirmDelete(item) {
    deleteTarget = item;
    showDeleteConfirm = true;
  }

  async function handleDelete() {
    if (!deleteTarget) return;
    error = '';
    try {
      await deleteFile(deleteTarget.key);
      showDeleteConfirm = false;
      deleteTarget = null;
      await browse(currentPath);
    } catch (e) {
      error = e.message;
    }
  }

  async function handleDownload(file) {
    try {
      await downloadFile(file.key, file.name);
    } catch (e) {
      error = e.message;
    }
  }

  function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }
</script>

<div
  class="space-y-4"
  ondrop={onDrop}
  ondragover={onDragOver}
  ondragenter={onDragEnter}
  ondragleave={onDragLeave}
  role="region"
>
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-lg font-semibold text-white flex items-center gap-2">
        <FolderOpen class="w-5 h-5 text-cyan-400" />
        Files
      </h2>
      <!-- Breadcrumb -->
      <nav class="flex items-center gap-1 mt-1 text-sm">
        <button
          class="text-slate-400 hover:text-white transition-colors flex items-center gap-1"
          onclick={() => navigateTo('')}
        >
          <Home class="w-3.5 h-3.5" />
          Home
        </button>
        {#each breadcrumbs() as crumb}
          <ChevronRight class="w-3 h-3 text-slate-600" />
          <button
            class="text-slate-400 hover:text-white transition-colors"
            onclick={() => navigateTo(crumb.path)}
          >
            {crumb.name}
          </button>
        {/each}
      </nav>
    </div>
    <div class="flex items-center gap-2">
      <button
        class="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-slate-800 text-slate-300 hover:bg-slate-700 rounded-lg transition-colors border border-slate-700"
        onclick={() => { showNewFolder = true; newFolderName = ''; }}
      >
        <FolderPlus class="w-4 h-4" />
        New Folder
      </button>
      <label
        class="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-cyan-600 text-white hover:bg-cyan-500 rounded-lg transition-colors cursor-pointer"
      >
        <Upload class="w-4 h-4" />
        Upload
        <input type="file" multiple class="hidden" onchange={onFileInput} />
      </label>
    </div>
  </div>

  <!-- Error -->
  {#if error}
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg px-4 py-2 text-sm text-red-400">
      {error}
    </div>
  {/if}

  <!-- Drag overlay -->
  {#if dragOver}
    <div class="fixed inset-0 bg-cyan-500/10 border-2 border-dashed border-cyan-400 z-50 flex items-center justify-center pointer-events-none">
      <div class="bg-slate-900 rounded-xl px-8 py-6 text-center shadow-2xl">
        <Upload class="w-10 h-10 text-cyan-400 mx-auto mb-2" />
        <p class="text-white font-medium">Drop to upload</p>
        <p class="text-sm text-slate-400">Files and folders to {currentPath || '/'}</p>
      </div>
    </div>
  {/if}

  <!-- Content -->
  {#if loading}
    <div class="flex items-center justify-center py-20">
      <div class="w-8 h-8 border-2 border-cyan-400 border-t-transparent rounded-full animate-spin"></div>
    </div>
  {:else if folders.length === 0 && files.length === 0 && !$isUploadingStore}
    <!-- Empty state with visible drop zone -->
    <div class="border-2 border-dashed rounded-xl p-12 text-center transition-colors {dragOver ? 'border-cyan-400 bg-cyan-950/30' : 'border-slate-700 hover:border-cyan-500/50'}">
      <Upload class="w-12 h-12 text-slate-600 mx-auto mb-3" />
      <p class="text-slate-400">Drag files or folders here</p>
      <p class="text-xs text-slate-600 mt-1">or click Upload above</p>
    </div>
  {:else}
    <div class="bg-slate-900 rounded-lg border border-slate-800 overflow-hidden">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-slate-800">
            <th class="text-left text-slate-400 font-medium px-4 py-2.5">Name</th>
            <th class="text-right text-slate-400 font-medium px-4 py-2.5 w-28">Size</th>
            <th class="text-left text-slate-400 font-medium px-4 py-2.5 w-32 hidden sm:table-cell">Date</th>
            <th class="text-right text-slate-400 font-medium px-4 py-2.5 w-24">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each folders as folder}
            <tr class="border-b border-slate-800/50 hover:bg-slate-800/30 cursor-pointer group" onclick={() => navigateTo(folder.path)}>
              <td class="px-4 py-2.5 text-slate-200">
                <div class="flex items-center gap-2.5">
                  <FolderOpen class="w-4 h-4 text-cyan-400 shrink-0" />
                  <span class="group-hover:text-white transition-colors">{folder.name}</span>
                </div>
              </td>
              <td class="px-4 py-2.5 text-right text-slate-500">&mdash;</td>
              <td class="px-4 py-2.5 text-slate-500 hidden sm:table-cell">&mdash;</td>
              <td class="px-4 py-2.5 text-right"></td>
            </tr>
          {/each}
          {#each files as file}
            <tr class="border-b border-slate-800/50 hover:bg-slate-800/30 group">
              <td class="px-4 py-2.5 text-slate-200">
                <div class="flex items-center gap-2.5">
                  {#if getFileIcon(file.name) === 'image'}
                    <Image class="w-4 h-4 text-purple-400 shrink-0" />
                  {:else if getFileIcon(file.name) === 'video'}
                    <Video class="w-4 h-4 text-pink-400 shrink-0" />
                  {:else if getFileIcon(file.name) === 'doc'}
                    <FileText class="w-4 h-4 text-blue-400 shrink-0" />
                  {:else if getFileIcon(file.name) === 'archive'}
                    <Archive class="w-4 h-4 text-amber-400 shrink-0" />
                  {:else}
                    <File class="w-4 h-4 text-slate-500 shrink-0" />
                  {/if}
                  <span class="truncate">{file.name}</span>
                </div>
              </td>
              <td class="px-4 py-2.5 text-right text-slate-400">{formatBytes(file.size)}</td>
              <td class="px-4 py-2.5 text-slate-500 text-xs hidden sm:table-cell">{file.created_at}</td>
              <td class="px-4 py-2.5 text-right">
                <div class="flex items-center justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    class="p-1 text-slate-500 hover:text-cyan-400 transition-colors"
                    title="Download"
                    onclick={(e) => { e.stopPropagation(); handleDownload(file); }}
                  >
                    <Download class="w-3.5 h-3.5" />
                  </button>
                  <button
                    class="p-1 text-slate-500 hover:text-red-400 transition-colors"
                    title="Delete"
                    onclick={(e) => { e.stopPropagation(); confirmDelete(file); }}
                  >
                    <Trash2 class="w-3.5 h-3.5" />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  <!-- Upload panel is now global in App.svelte -->

  <!-- New Folder Modal -->
  {#if showNewFolder}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="fixed inset-0 bg-black/60 z-50 flex items-center justify-center" onclick={() => showNewFolder = false}>
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div class="bg-slate-900 rounded-xl border border-slate-700 p-6 w-96 shadow-xl" onclick={(e) => e.stopPropagation()}>
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-white font-medium">New Folder</h3>
          <button class="text-slate-500 hover:text-slate-300" onclick={() => showNewFolder = false}>
            <X class="w-4 h-4" />
          </button>
        </div>
        <input
          type="text"
          placeholder="Folder name"
          class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
          bind:value={newFolderName}
          onkeydown={(e) => { if (e.key === 'Enter') handleCreateFolder(); }}
        />
        <div class="flex justify-end gap-2 mt-4">
          <button
            class="px-3 py-1.5 text-sm text-slate-400 hover:text-white transition-colors"
            onclick={() => showNewFolder = false}
          >
            Cancel
          </button>
          <button
            class="px-3 py-1.5 text-sm bg-cyan-600 text-white rounded-lg hover:bg-cyan-500 transition-colors"
            onclick={handleCreateFolder}
          >
            Create
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Delete Confirmation Modal -->
  {#if showDeleteConfirm && deleteTarget}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="fixed inset-0 bg-black/60 z-50 flex items-center justify-center" onclick={() => showDeleteConfirm = false}>
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div class="bg-slate-900 rounded-xl border border-slate-700 p-6 w-96 shadow-xl" onclick={(e) => e.stopPropagation()}>
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-white font-medium">Delete File</h3>
          <button class="text-slate-500 hover:text-slate-300" onclick={() => showDeleteConfirm = false}>
            <X class="w-4 h-4" />
          </button>
        </div>
        <p class="text-sm text-slate-400">
          Are you sure you want to delete <span class="text-white font-medium">{deleteTarget.name}</span>?
          This action cannot be undone.
        </p>
        <div class="flex justify-end gap-2 mt-4">
          <button
            class="px-3 py-1.5 text-sm text-slate-400 hover:text-white transition-colors"
            onclick={() => showDeleteConfirm = false}
          >
            Cancel
          </button>
          <button
            class="px-3 py-1.5 text-sm bg-red-600 text-white rounded-lg hover:bg-red-500 transition-colors"
            onclick={handleDelete}
          >
            Delete
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
