import { Badge, Tooltip } from 'antd';
import type { ServiceStatus } from '../types';

const MAP: Record<ServiceStatus, { status: 'success' | 'processing' | 'warning' | 'error' | 'default'; text: string }> = {
  running: { status: 'success', text: '运行中' },
  starting: { status: 'processing', text: '启动中' },
  stopping: { status: 'processing', text: '停止中' },
  backoff: { status: 'warning', text: '等待重启' },
  failed: { status: 'error', text: '异常退出' },
  stopped: { status: 'default', text: '已停止' },
};

export default function StatusBadge({
  status,
  error,
}: {
  status: ServiceStatus;
  error?: string | null;
}) {
  const cfg = MAP[status] ?? MAP.stopped;
  const badge = <Badge status={cfg.status} text={cfg.text} />;
  if (status === 'failed' && error) {
    return <Tooltip title={error}>{badge}</Tooltip>;
  }
  return badge;
}
