<script>
  import { X } from 'lucide-svelte';

  let { title = '', show = false, onclose = () => {}, children } = $props();
</script>

{#if show}
  <div class="fixed inset-0 z-50 flex items-center justify-center">
    <div class="absolute inset-0 bg-black/60" onclick={onclose}></div>
    <div class="relative bg-slate-900 border border-slate-700 rounded-xl shadow-2xl w-full max-w-lg mx-4 max-h-[80vh] overflow-y-auto">
      <div class="flex items-center justify-between px-6 py-4 border-b border-slate-800">
        <h2 class="text-lg font-semibold text-white">{title}</h2>
        <button onclick={onclose} class="text-slate-400 hover:text-white transition-colors">
          <X class="w-5 h-5" />
        </button>
      </div>
      <div class="p-6">
        {@render children()}
      </div>
    </div>
  </div>
{/if}
