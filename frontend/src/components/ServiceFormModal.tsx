import { useEffect } from 'react';
import {
  Button,
  Checkbox,
  Form,
  Input,
  Modal,
  Space,
  Typography,
} from 'antd';
import { MinusCircleOutlined, PlusOutlined } from '@ant-design/icons';
import type { ServiceInfo } from '../types';
import { shellSplit } from '../utils';

export interface ServiceFormValues {
  name: string;
  command: string;
  args: string;
  cwd: string;
  env: { key: string; value: string }[];
  autostart: boolean;
  auto_restart: boolean;
}

export interface ServiceSubmitPayload {
  name: string;
  command: string;
  args: string[];
  cwd: string;
  env: Record<string, string>;
  autostart: boolean;
  auto_restart: boolean;
}

export function toSubmitPayload(v: ServiceFormValues): ServiceSubmitPayload {
  const env: Record<string, string> = {};
  for (const item of v.env ?? []) {
    if (item.key?.trim()) env[item.key.trim()] = item.value ?? '';
  }
  return {
    name: v.name,
    command: v.command,
    args: shellSplit(v.args ?? ''),
    cwd: v.cwd ?? '',
    env,
    autostart: v.autostart ?? false,
    auto_restart: v.auto_restart ?? false,
  };
}

export default function ServiceFormModal({
  open,
  initial,
  submitting,
  onOk,
  onCancel,
}: {
  open: boolean;
  initial: ServiceInfo | null;
  submitting: boolean;
  onOk: (payload: ServiceSubmitPayload) => void;
  onCancel: () => void;
}) {
  const [form] = Form.useForm<ServiceFormValues>();

  useEffect(() => {
    if (!open) return;
    if (initial) {
      form.setFieldsValue({
        name: initial.name,
        command: initial.command,
        args: initial.args.join(' '),
        cwd: initial.cwd,
        env: Object.entries(initial.env ?? {}).map(([key, value]) => ({ key, value })),
        autostart: initial.autostart,
        auto_restart: initial.auto_restart,
      });
    } else {
      form.resetFields();
      form.setFieldsValue({
        name: '',
        command: '',
        args: '',
        cwd: '',
        env: [],
        autostart: false,
        auto_restart: false,
      });
    }
  }, [open, initial, form]);

  return (
    <Modal
      title={initial ? '编辑服务' : '添加服务'}
      open={open}
      onCancel={onCancel}
      confirmLoading={submitting}
      onOk={async () => {
        const values = await form.validateFields();
        onOk(toSubmitPayload(values));
      }}
      okText="保存"
      cancelText="取消"
      destroyOnClose
      width={560}
    >
      <Form form={form} layout="vertical" autoComplete="off">
        <Form.Item
          name="name"
          label="名称"
          rules={[{ required: true, message: '请输入服务名称' }]}
        >
          <Input placeholder="dufs" />
        </Form.Item>
        <Form.Item
          name="command"
          label="命令"
          rules={[{ required: true, message: '请输入启动命令' }]}
        >
          <Input placeholder="dufs" />
        </Form.Item>
        <Form.Item name="args" label="参数">
          <Input placeholder="-p 5001 --allow-upload" />
        </Form.Item>
        <Form.Item name="cwd" label="工作目录">
          <Input placeholder="/data" />
        </Form.Item>
        <Form.Item label="环境变量" style={{ marginBottom: 0 }}>
          <Form.List name="env">
            {(fields, { add, remove }) => (
              <>
                {fields.map((field) => (
                  <Space key={field.key} align="baseline" style={{ display: 'flex', marginBottom: 4 }}>
                    <Form.Item
                      name={[field.name, 'key']}
                      rules={[{ required: true, message: '变量名不能为空' }]}
                      style={{ marginBottom: 0 }}
                    >
                      <Input placeholder="KEY" style={{ width: 160 }} />
                    </Form.Item>
                    <Form.Item
                      name={[field.name, 'value']}
                      style={{ marginBottom: 0, flex: 1 }}
                    >
                      <Input placeholder="value" />
                    </Form.Item>
                    <MinusCircleOutlined onClick={() => remove(field.name)} />
                  </Space>
                ))}
                <Button type="dashed" block icon={<PlusOutlined />} onClick={() => add()}>
                  添加变量
                </Button>
              </>
            )}
          </Form.List>
        </Form.Item>
        <Space size="large">
          <Form.Item name="autostart" valuePropName="checked" style={{ marginBottom: 0 }}>
            <Checkbox>随管理器自启</Checkbox>
          </Form.Item>
          <Form.Item name="auto_restart" valuePropName="checked" style={{ marginBottom: 0 }}>
            <Checkbox>异常自动重启</Checkbox>
          </Form.Item>
        </Space>
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>
          参数支持引号，例如 -p 5001 --path "/my dir"
        </Typography.Text>
      </Form>
    </Modal>
  );
}
