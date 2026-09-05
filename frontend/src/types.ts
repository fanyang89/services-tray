export type ServiceStatus =
  | 'stopped'
  | 'starting'
  | 'running'
  | 'stopping'
  | 'backoff'
  | 'failed';

export interface ServiceConfig {
  name: string;
  command: string;
  args: string[];
  cwd: string;
  env: Record<string, string>;
  autostart: boolean;
  auto_restart: boolean;
}

export interface ServiceInfo extends ServiceConfig {
  id: string;
  status: ServiceStatus;
  pid: number | null;
  /** Unix epoch milliseconds while running. */
  started_at: number | null;
  exit_code: number | null;
  last_error: string | null;
}

export interface LogLine {
  seq: number;
  ts: number;
  stream: 'stdout' | 'stderr' | 'system';
  line: string;
}
