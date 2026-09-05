import { useEffect, useRef, useState } from 'react';
import { Button, Drawer, Empty, Switch } from 'antd';
import { ClearOutlined, SyncOutlined } from '@ant-design/icons';
import type { LogLine, ServiceInfo } from '../types';
import { fetchLogs } from '../api';
import { formatTime } from '../utils';

const MAX_VIEW_LINES = 3000;

export default function LogDrawer({
  service,
  open,
  onClose,
}: {
  service: ServiceInfo | null;
  open: boolean;
  onClose: () => void;
}) {
  const [lines, setLines] = useState<LogLine[]>([]);
  const [follow, setFollow] = useState(true);
  const boxRef = useRef<HTMLDivElement>(null);
  const lastSeqRef = useRef(0);

  useEffect(() => {
    if (!open || !service) return;
    setLines([]);
    lastSeqRef.current = 0;

    const es = new EventSource(`/api/services/${service.id}/logs/stream?tail=500`);
    es.addEventListener('log', (ev) => {
      try {
        const line: LogLine = JSON.parse((ev as MessageEvent).data);
        if (line.seq <= lastSeqRef.current) return;
        lastSeqRef.current = line.seq;
        setLines((prev) => {
          const base = prev.length > MAX_VIEW_LINES ? prev.slice(-MAX_VIEW_LINES) : prev.slice();
          base.push(line);
          return base;
        });
      } catch {
        // ignore malformed frames
      }
    });
    return () => es.close();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, service?.id]);

  useEffect(() => {
    if (follow && boxRef.current) {
      boxRef.current.scrollTop = boxRef.current.scrollHeight;
    }
  }, [lines, follow]);

  const refresh = async () => {
    if (!service) return;
    const fresh = await fetchLogs(service.id, 500);
    lastSeqRef.current = fresh.length > 0 ? fresh[fresh.length - 1].seq : 0;
    setLines(fresh);
  };

  return (
    <Drawer
      title={`日志 · ${service?.name ?? ''}`}
      open={open && !!service}
      onClose={onClose}
      width="min(920px, 92vw)"
      styles={{ body: { position: 'relative', padding: 12 } }}
      extra={
        <div className="log-toolbar">
          <span style={{ fontSize: 13 }}>跟随最新</span>
          <Switch aria-label="跟随最新日志" size="small" checked={follow} onChange={setFollow} />
          <Button size="small" icon={<SyncOutlined />} onClick={refresh}>
            刷新
          </Button>
          <Button size="small" icon={<ClearOutlined />} onClick={() => setLines([])}>
            清屏
          </Button>
        </div>
      }
    >
      <div className="log-box" ref={boxRef}>
        {lines.length === 0 ? (
          <Empty description="暂无日志" style={{ marginTop: 80 }} />
        ) : (
          lines.map((l) => (
            <div key={l.seq} className={`log-line log-${l.stream}`}>
              <span className="log-time">{formatTime(l.ts)}</span>
              <span className="log-text">{l.line}</span>
            </div>
          ))
        )}
      </div>
    </Drawer>
  );
}
