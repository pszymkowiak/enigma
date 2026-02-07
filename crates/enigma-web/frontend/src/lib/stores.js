import { writable } from 'svelte/store';

export const token = writable(localStorage.getItem('enigma_token') || '');
export const currentPage = writable('dashboard');

token.subscribe(value => {
  if (value) {
    localStorage.setItem('enigma_token', value);
  } else {
    localStorage.removeItem('enigma_token');
  }
});
