import { useState } from 'react';
import { Layout, Tag, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import ServicesTable from './components/ServicesTable';
import LogDrawer from './components/LogDrawer';
import { fetchHealth } from './api';
import type { ServiceInfo } from './types';

export default function App() {
  const [logService, setLogService] = useState<ServiceInfo | null>(null);
  const health = useQuery({
    queryKey: ['health'],
    queryFn: fetchHealth,
    refetchInterval: 60_000,
    retry: false,
  });

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Layout.Header
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          background: '#001529',
        }}
      >
        <Typography.Title level={4} style={{ color: '#fff', margin: 0 }}>
          服务管理器
        </Typography.Title>
        {health.data && (
          <Tag color="blue" style={{ marginRight: 0 }}>
            v{health.data.version}
          </Tag>
        )}
      </Layout.Header>
      <Layout.Content
        style={{
          padding: 24,
          maxWidth: 1200,
          margin: '0 auto',
          width: '100%',
          boxSizing: 'border-box',
        }}
      >
        <ServicesTable onShowLogs={setLogService} />
      </Layout.Content>
      <LogDrawer
        service={logService}
        open={!!logService}
        onClose={() => setLogService(null)}
      />
    </Layout>
  );
}
