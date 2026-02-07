<script>
  import { uploadQueue, showUploadPanel, isUploading, uploadDone, uploadTotal, clearUploadQueue, retryFailedUploads } from '../lib/stores.js';
  import { File, FileText, Image, Video, Archive, X, Check, AlertCircle } from 'lucide-svelte';
  import { onMount, onDestroy } from 'svelte';

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

  // Prevent closing tab during upload
  function onBeforeUnload(e) {
    if ($isUploading) {
      e.preventDefault();
      e.returnValue = '';
    }
  }

  onMount(() => {
    window.addEventListener('beforeunload', onBeforeUnload);
  });

  onDestroy(() => {
    window.removeEventListener('beforeunload', onBeforeUnload);
  });
</script>

{#if $uploadQueue.length > 0 && $showUploadPanel}
  <div class="fixed bottom-4 right-4 w-96 bg-slate-900 border border-slate-700 rounded-xl shadow-2xl z-40 max-h-80 flex flex-col">
    <!-- Header -->
    <div class="flex items-center justify-between px-4 py-3 border-b border-slate-800">
      <span class="text-sm text-white font-medium">
        {#if $isUploading}
          Uploading {$uploadDone}/{$uploadTotal}
        {:else}
          Uploaded {$uploadDone}/{$uploadTotal}
        {/if}
      </span>
      <div class="flex items-center gap-2">
        {#if !$isUploading && $uploadQueue.some(f => f.status === 'error')}
          <button
            class="text-xs text-cyan-400 hover:text-cyan-300 transition-colors"
            onclick={retryFailedUploads}
          >
            Retry failed
          </button>
        {/if}
        <button onclick={clearUploadQueue} class="text-slate-500 hover:text-white transition-colors">
          <X class="w-4 h-4" />
        </button>
      </div>
    </div>
    <!-- Overall progress bar -->
    {#if $isUploading}
      {@const overallProgress = $uploadTotal > 0 ? $uploadQueue.reduce((sum, f) => sum + (f.status === 'done' ? 1 : f.progress), 0) / $uploadTotal : 0}
      <div class="h-0.5 bg-slate-800">
        <div class="h-0.5 bg-cyan-500 transition-all duration-300" style="width: {overallProgress * 100}%"></div>
      </div>
    {/if}
    <!-- File list -->
    <div class="overflow-y-auto flex-1 px-4 py-2 space-y-2">
      {#each $uploadQueue as item (item.id)}
        <div class="flex items-center gap-3 py-1">
          {#if getFileIcon(item.name) === 'image'}
            <Image class="w-4 h-4 text-purple-400 shrink-0" />
          {:else if getFileIcon(item.name) === 'video'}
            <Video class="w-4 h-4 text-pink-400 shrink-0" />
          {:else if getFileIcon(item.name) === 'doc'}
            <FileText class="w-4 h-4 text-blue-400 shrink-0" />
          {:else if getFileIcon(item.name) === 'archive'}
            <Archive class="w-4 h-4 text-amber-400 shrink-0" />
          {:else}
            <File class="w-4 h-4 text-slate-500 shrink-0" />
          {/if}
          <div class="flex-1 min-w-0">
            <p class="text-xs text-slate-300 truncate" title={item.relativePath}>{item.relativePath}</p>
            <div class="h-1 bg-slate-800 rounded-full mt-1">
              <div class="h-1 rounded-full transition-all duration-300"
                   class:bg-cyan-500={item.status === 'uploading' || item.status === 'pending'}
                   class:bg-green-500={item.status === 'done'}
                   class:bg-red-500={item.status === 'error'}
                   style="width: {item.progress * 100}%"></div>
            </div>
            {#if item.status === 'error'}
              <p class="text-[10px] text-red-400 mt-0.5 truncate">{item.error}</p>
            {/if}
          </div>
          {#if item.status === 'done'}
            <Check class="w-4 h-4 text-green-500 shrink-0" />
          {:else if item.status === 'error'}
            <AlertCircle class="w-4 h-4 text-red-500 shrink-0" />
          {:else if item.status === 'uploading'}
            <div class="w-4 h-4 border-2 border-cyan-400 border-t-transparent rounded-full animate-spin shrink-0"></div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}
