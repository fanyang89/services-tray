import React from 'react';
import ReactDOM from 'react-dom/client';
import { App as AntdApp, ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import dayjs from 'dayjs';
import 'dayjs/locale/zh-cn';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import App from './App';
import './index.css';

dayjs.locale('zh-cn');

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ConfigProvider locale={zhCN} theme={{
      token: {
        colorPrimary: '#c35b35',
        colorInfo: '#c35b35',
        colorSuccess: '#538065',
        colorError: '#b64c45',
        colorWarning: '#b27c30',
        colorText: '#302e2a',
        colorTextSecondary: '#7c776e',
        colorBorder: '#e3ddd2',
        colorBgContainer: '#fffdf9',
        colorBgElevated: '#fffdf9',
        borderRadius: 10,
        controlHeight: 36,
        fontFamily: "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Microsoft YaHei', sans-serif",
        boxShadow: '0 12px 40px rgba(57, 43, 26, 0.08)',
      },
      components: {
        Button: { primaryShadow: '0 3px 8px rgba(160, 73, 39, 0.12)', fontWeight: 500 },
        Input: { activeShadow: '0 0 0 3px rgba(195, 91, 53, 0.1)' },
      },
    }}>
      <QueryClientProvider client={queryClient}>
        <AntdApp>
          <App />
        </AntdApp>
      </QueryClientProvider>
    </ConfigProvider>
  </React.StrictMode>,
);
