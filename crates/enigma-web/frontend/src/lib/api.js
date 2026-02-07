import { token } from './stores.js';
import { get } from 'svelte/store';

const BASE = '/api';

async function request(path, options = {}) {
  const t = get(token);
  const headers = {
    'Content-Type': 'application/json',
    ...(t ? { Authorization: `Bearer ${t}` } : {}),
    ...options.headers,
  };

  const res = await fetch(`${BASE}${path}`, { ...options, headers });

  if (res.status === 401) {
    token.set('');
    throw new Error('Unauthorized');
  }

  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }

  return res.json();
}

export async function login(username, password) {
  const data = await request('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ username, password }),
  });
  token.set(data.token);
  return data;
}

export async function getStatus() {
  return request('/status');
}

export async function getProviders() {
  return request('/storage/providers');
}

export async function getChunkStats() {
  return request('/storage/chunks/stats');
}

export async function getBackups() {
  return request('/storage/backups');
}

export async function getNamespaces() {
  return request('/namespaces');
}

export async function getObjects(namespace) {
  return request(`/namespaces/${namespace}/objects`);
}

export async function getCluster() {
  return request('/cluster');
}
