import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, App, Button, Input, Skeleton, Switch, Tooltip } from 'antd';
import {
  ApiOutlined,
  SearchOutlined,
  FolderOpenOutlined,
  ClockCircleOutlined,
  CaretRightOutlined,
  DeleteOutlined,
  EditOutlined,
  FileTextOutlined,
  PauseOutlined,
  PlusOutlined,
  RedoOutlined,
} from '@ant-design/icons';
import type { ServiceInfo } from '../types';
import {
  createService,
  deleteService,
  listServices,
  restartService,
  setAutostart,
  startService,
  stopService,
  updateService,
} from '../api';
import { displayCommand, formatUptime } from '../utils';
import StatusBadge from './StatusBadge';
import ServiceFormModal, { type ServiceSubmitPayload } from './ServiceFormModal';

export default function ServiceDashboard({ onShowLogs }: { onShowLogs: (svc: ServiceInfo) => void }) {
  const { message, modal } = App.useApp();
  const queryClient = useQueryClient();
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<ServiceInfo | null>(null);

  const { data: services = [], isLoading, isError, error, refetch } = useQuery({
    queryKey: ['services'],
    queryFn: listServices,
    refetchInterval: 2000,
  });

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ['services'] });

  const act = useMutation({
    mutationFn: ({ id, action }: { id: string; action: 'start' | 'stop' | 'restart' }) => {
      if (action === 'start') return startService(id);
      if (action === 'stop') return stopService(id);
      return restartService(id);
    },
    onSuccess: (_data, vars) => {
      message.success(
        vars.action === 'start' ? '已启动' : vars.action === 'stop' ? '已停止' : '已重启',
      );
      invalidate();
    },
    onError: (e: Error) => message.error(e.message),
  });

  const toggleAutostart = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) => setAutostart(id, enabled),
    onSuccess: () => invalidate(),
    onError: (e: Error) => {
      message.error(e.message);
      invalidate();
    },
  });

  const submitForm = useMutation({
    mutationFn: (payload: { id?: string; values: ServiceSubmitPayload }) =>
      payload.id
        ? updateService(payload.id, payload.values)
        : createService(payload.values),
    onSuccess: (_data, payload) => {
      message.success(payload.id ? '已保存' : '已添加');
      setFormOpen(false);
      setEditing(null);
      invalidate();
    },
    onError: (e: Error) => message.error(e.message),
  });

  const remove = useMutation({
    mutationFn: (id: string) => deleteService(id),
    onSuccess: () => {
      message.success('已删除');
      invalidate();
    },
    onError: (e: Error) => message.error(e.message),
  });

  const confirmDelete = (svc: ServiceInfo) => {
    modal.confirm({
      title: `删除服务「${svc.name}」？`,
      content: svc.status === 'running' ? '服务正在运行，将先停止再删除。' : '删除后不可恢复。',
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: () => remove.mutateAsync(svc.id),
    });
  };

  const busy = (s: ServiceInfo) =>
    s.status === 'running' ||
    s.status === 'starting' ||
    s.status === 'stopping' ||
    s.status === 'backoff';

  const [search, setSearch] = useState('');
  const [filter, setFilter] = useState<'all' | 'running' | 'attention'>('all');
  // Query data may be structurally unchanged while the uptime keeps ticking.
  const [, tick] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => tick((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const running = services.filter((s) => s.status === 'running').length;
  const attention = services.filter((s) => s.status === 'failed' || s.status === 'backoff').length;
  const visible = services.filter((s) => {
    const matches = `${s.name} ${s.command} ${s.args.join(' ')}`.toLowerCase().includes(search.toLowerCase());
    return matches && (filter === 'all' || (filter === 'running' ? s.status === 'running' : s.status === 'failed' || s.status === 'backoff'));
  });
  const addService = () => { setEditing(null); setFormOpen(true); };

  return (
    <section id="top" aria-labelledby="page-title">
      <div className="page-heading">
        <div>
          <p className="eyebrow">YOUR LITTLE CONTROL ROOM</p>
          <h1 id="page-title">服务，各就各位<span className="accent-dot">.</span></h1>
          <p className="page-subtitle">日常的小服务，都在这里。</p>
        </div>
        <Button type="primary" size="large" icon={<PlusOutlined />} onClick={addService}>添加服务</Button>
      </div>

      <div className="overview" aria-label="服务概览">
        <div><span className="stat-label">全部服务</span><strong>{isLoading || isError ? '—' : services.length.toString().padStart(2, '0')}</strong></div>
        <div><span className="stat-label"><i className="tiny-dot green" />运行中</span><strong>{isLoading || isError ? '—' : running.toString().padStart(2, '0')}</strong></div>
        <div><span className="stat-label"><i className="tiny-dot orange" />需要关注</span><strong>{isLoading || isError ? '—' : attention.toString().padStart(2, '0')}</strong></div>
        <span className="overview-symbol" aria-hidden="true">✳</span>
      </div>

      <div className="service-toolbar">
        <div className="filter-tabs" role="group" aria-label="服务状态筛选">
          {([['all', '全部'], ['running', '运行中'], ['attention', '需关注']] as const).map(([key, label]) => (
            <button key={key} type="button" aria-pressed={filter === key} className={filter === key ? 'selected' : ''} onClick={() => setFilter(key)}>{label}</button>
          ))}
        </div>
        <Input className="service-search" prefix={<SearchOutlined />} placeholder="搜索名称或命令" aria-label="搜索服务" allowClear value={search} onChange={(e) => setSearch(e.target.value)} />
      </div>

      {isError ? (
        <Alert type="error" showIcon message="暂时无法获取服务" description={error.message} action={<Button onClick={() => void refetch()}>重试</Button>} />
      ) : isLoading ? (
        <div className="service-grid" aria-label="正在加载服务">{[0, 1, 2].map((i) => <div className="service-card" key={i}><Skeleton active paragraph={{ rows: 4 }} /></div>)}</div>
      ) : visible.length === 0 ? (
        <div className="empty-state">
          <span className="empty-icon"><ApiOutlined /></span>
          <h2>{services.length === 0 ? '给你的第一个服务，留个位置。' : '没有匹配的服务'}</h2>
          <p>{services.length === 0 ? 'dufs、开发服务器，或你的下一个小项目。' : '换个关键词，或者看看全部服务。'}</p>
          {services.length === 0 ? <Button type="primary" icon={<PlusOutlined />} onClick={addService}>添加服务</Button> : <Button onClick={() => { setSearch(''); setFilter('all'); }}>重置筛选</Button>}
        </div>
      ) : (
        <div className="service-grid">
          {visible.map((s) => (
            <article className={`service-card state-${s.status}`} key={s.id} aria-label={s.name}>
              <div className="service-card-heading">
                <span className="service-icon"><ApiOutlined /></span>
                <StatusBadge status={s.status} error={s.last_error} />
              </div>
              <h2 title={s.name}>{s.name}</h2>
              <div className="command-line" title={displayCommand(s.command, s.args)}><span aria-hidden="true">$</span><code>{displayCommand(s.command, s.args)}</code></div>
              <div className="service-metadata">
                <span title={s.cwd || '继承管理器目录'}><FolderOpenOutlined /> {s.cwd || '默认目录'}</span>
                <span><ClockCircleOutlined /> {s.status === 'running' ? formatUptime(s.started_at) : s.exit_code != null ? `退出码 ${s.exit_code}` : '未在运行'}</span>
              </div>
              <div className="autostart-row">
                <label htmlFor={`autostart-${s.id}`}>随管理器自启</label>
                <Switch id={`autostart-${s.id}`} size="small" aria-label={`${s.name} 随管理器自启`} checked={s.autostart} loading={toggleAutostart.isPending && toggleAutostart.variables?.id === s.id} onChange={(enabled) => toggleAutostart.mutate({ id: s.id, enabled })} />
              </div>
              <div className="card-actions">
                <div className="lifecycle-actions">
                  <Button type={busy(s) ? 'default' : 'primary'} danger={busy(s)} icon={busy(s) ? <PauseOutlined /> : <CaretRightOutlined />} disabled={act.isPending} loading={act.isPending && act.variables?.id === s.id} onClick={() => act.mutate({ id: s.id, action: busy(s) ? 'stop' : 'start' })}>{busy(s) ? '停止' : '启动'}</Button>
                  {s.status === 'running' && <Tooltip title="重启"><Button aria-label={`重启 ${s.name}`} icon={<RedoOutlined />} disabled={act.isPending} onClick={() => act.mutate({ id: s.id, action: 'restart' })} /></Tooltip>}
                </div>
                <div className="secondary-actions">
                  <Tooltip title="日志"><Button type="text" aria-label={`查看 ${s.name} 日志`} icon={<FileTextOutlined />} onClick={() => onShowLogs(s)} /></Tooltip>
                  <Tooltip title="编辑"><Button type="text" aria-label={`编辑 ${s.name}`} icon={<EditOutlined />} onClick={() => { setEditing(s); setFormOpen(true); }} /></Tooltip>
                  <Tooltip title="删除"><Button type="text" danger aria-label={`删除 ${s.name}`} icon={<DeleteOutlined />} onClick={() => confirmDelete(s)} /></Tooltip>
                </div>
              </div>
              <div className="card-footnote"><span>{s.pid ? `PID ${s.pid}` : 'PROCESS ID · —'}</span><span>{s.auto_restart ? '异常自动重启' : '手动重启'}</span></div>
            </article>
          ))}
        </div>
      )}

      <ServiceFormModal
        open={formOpen}
        initial={editing}
        submitting={submitForm.isPending}
        onOk={(values) => {
          if (editing) submitForm.mutate({ id: editing.id, values });
          else submitForm.mutate({ values });
        }}
        onCancel={() => {
          setFormOpen(false);
          setEditing(null);
        }}
      />
    </section>
  );
}
