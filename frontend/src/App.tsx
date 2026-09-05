import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { ApiOutlined, ArrowUpOutlined } from '@ant-design/icons';
import ServiceDashboard from './components/ServiceDashboard';
import LogDrawer from './components/LogDrawer';
import { fetchHealth } from './api';
import type { ServiceInfo } from './types';

export default function App() {
  const [logService, setLogService] = useState<ServiceInfo | null>(null);
  const health = useQuery({
    queryKey: ['health'], queryFn: fetchHealth, refetchInterval: 15_000, retry: false,
  });

  return (
    <div className="app-shell">
      <header className="app-header">
        <a className="brand" href="/" aria-label="服务管理器首页">
          <span className="brand-icon"><ApiOutlined /></span>
          <span>小驻<span className="brand-caption">SERVICES MANAGER</span></span>
        </a>
        <div className="connection-status" role="status">
          <span className={`connection-dot ${health.isError ? 'offline' : health.data ? 'online' : ''}`} />
          {health.isError ? '连接中断' : health.data ? '管理器已连接' : '正在连接'}
          {health.data && <span className="version">v{health.data.version}</span>}
        </div>
      </header>
      <main className="main-content">
        <ServiceDashboard onShowLogs={setLogService} />
      </main>
      <footer className="app-footer">
        <span>小驻 · 让服务安静运行</span>
        <a href="#top">回到顶部 <ArrowUpOutlined /></a>
      </footer>
      <LogDrawer service={logService} open={!!logService} onClose={() => setLogService(null)} />
    </div>
  );
}
