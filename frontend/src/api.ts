import type { LogLine, ServiceConfig, ServiceInfo } from './types';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`/api${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
  if (!res.ok) {
    let message = `请求失败 (${res.status})`;
    try {
      const body = await res.json();
      if (body?.error) message = body.error;
    } catch {
      // keep default message
    }
    throw new Error(message);
  }
  return res.json() as Promise<T>;
}

export const fetchHealth = () =>
  request<{ status: string; version: string }>('/health');

export const listServices = () => request<ServiceInfo[]>('/services');

export const createService = (input: ServiceConfig) =>
  request<ServiceInfo>('/services', { method: 'POST', body: JSON.stringify(input) });

export const updateService = (id: string, input: ServiceConfig) =>
  request<ServiceInfo>(`/services/${id}`, { method: 'PUT', body: JSON.stringify(input) });

export const deleteService = (id: string) =>
  request<{ ok: boolean }>(`/services/${id}`, { method: 'DELETE' });

export const setAutostart = (id: string, enabled: boolean) =>
  request<ServiceInfo>(`/services/${id}/autostart`, {
    method: 'PUT',
    body: JSON.stringify({ enabled }),
  });

export const startService = (id: string) =>
  request<ServiceInfo>(`/services/${id}/start`, { method: 'POST' });

export const stopService = (id: string) =>
  request<ServiceInfo>(`/services/${id}/stop`, { method: 'POST' });

export const restartService = (id: string) =>
  request<ServiceInfo>(`/services/${id}/restart`, { method: 'POST' });

export const fetchLogs = (id: string, tail = 500, after?: number) =>
  request<LogLine[]>(
    `/services/${id}/logs?tail=${tail}${after != null ? `&after=${after}` : ''}`,
  );
