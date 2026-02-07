import { writable } from 'svelte/store';

export const token = writable(localStorage.getItem('enigma_token') || '');
export const currentPage = writable('dashboard');
export const sidebarCollapsed = writable(localStorage.getItem('enigma_sidebar') === 'collapsed');

token.subscribe(value => {
  if (value) {
    localStorage.setItem('enigma_token', value);
  } else {
    localStorage.removeItem('enigma_token');
  }
});

sidebarCollapsed.subscribe(value => {
  localStorage.setItem('enigma_sidebar', value ? 'collapsed' : 'expanded');
});
