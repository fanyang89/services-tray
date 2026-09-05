import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { App, Button, Card, Space, Switch, Table, Tag, Tooltip, Typography } from 'antd';
import {
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

export default function ServicesTable({ onShowLogs }: { onShowLogs: (svc: ServiceInfo) => void }) {
  const { message, modal } = App.useApp();
  const queryClient = useQueryClient();
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<ServiceInfo | null>(null);

  const { data: services = [], isLoading } = useQuery({
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

  return (
    <Card
      title="服务列表"
      extra={
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          添加服务
        </Button>
      }
    >
      <Table<ServiceInfo>
        rowKey="id"
        dataSource={services}
        loading={isLoading}
        pagination={false}
        size="middle"
        columns={[
          {
            title: '状态',
            dataIndex: 'status',
            width: 110,
            render: (_: unknown, s: ServiceInfo) => (
              <StatusBadge status={s.status} error={s.last_error} />
            ),
          },
          {
            title: '名称',
            dataIndex: 'name',
            render: (_: unknown, s: ServiceInfo) => (
              <Space direction="vertical" size={0}>
                <Typography.Text strong>{s.name}</Typography.Text>
                <Typography.Text code style={{ fontSize: 12 }} copyable>
                  {displayCommand(s.command, s.args)}
                </Typography.Text>
              </Space>
            ),
          },
          {
            title: 'PID / 运行时长',
            width: 160,
            render: (_: unknown, s: ServiceInfo) =>
              s.status === 'running' && s.pid ? (
                <Space direction="vertical" size={0}>
                  <Typography.Text>PID {s.pid}</Typography.Text>
                  <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                    {formatUptime(s.started_at)}
                  </Typography.Text>
                </Space>
              ) : s.exit_code != null ? (
                <Tooltip title={s.status === 'failed' ? '上次退出码' : undefined}>
                  <Tag>exit {s.exit_code}</Tag>
                </Tooltip>
              ) : (
                <Typography.Text type="secondary">-</Typography.Text>
              ),
          },
          {
            title: '随管理器自启',
            dataIndex: 'autostart',
            width: 130,
            render: (_: unknown, s: ServiceInfo) => (
              <Switch
                size="small"
                checked={s.autostart}
                loading={toggleAutostart.isPending && toggleAutostart.variables?.id === s.id}
                onChange={(enabled) => toggleAutostart.mutate({ id: s.id, enabled })}
              />
            ),
          },
          {
            title: '操作',
            width: 300,
            render: (_: unknown, s: ServiceInfo) => (
              <Space size={4} wrap>
                {busy(s) ? (
                  <>
                    <Button
                      size="small"
                      danger
                      icon={<PauseOutlined />}
                      loading={act.isPending && act.variables?.id === s.id && act.variables?.action === 'stop'}
                      onClick={() => act.mutate({ id: s.id, action: 'stop' })}
                    >
                      停止
                    </Button>
                    {s.status === 'running' && (
                      <Button
                        size="small"
                        icon={<RedoOutlined />}
                        onClick={() => act.mutate({ id: s.id, action: 'restart' })}
                      >
                        重启
                      </Button>
                    )}
                  </>
                ) : (
                  <Button
                    size="small"
                    type="primary"
                    ghost
                    icon={<CaretRightOutlined />}
                    loading={act.isPending && act.variables?.id === s.id && act.variables?.action === 'start'}
                    onClick={() => act.mutate({ id: s.id, action: 'start' })}
                  >
                    启动
                  </Button>
                )}
                <Button size="small" icon={<FileTextOutlined />} onClick={() => onShowLogs(s)}>
                  日志
                </Button>
                <Button
                  size="small"
                  icon={<EditOutlined />}
                  onClick={() => {
                    setEditing(s);
                    setFormOpen(true);
                  }}
                />
                <Button size="small" danger icon={<DeleteOutlined />} onClick={() => confirmDelete(s)} />
              </Space>
            ),
          },
        ]}
      />

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
    </Card>
  );
}
