# 监控告警

> **阅读时间**: 15 分钟  
> **适用对象**: 运维工程师

---

## 1. 监控架构

```
┌─────────────────────────────────────────────────────────┐
│                    监控架构                              │
│                                                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │ Node 1   │  │ Node 2   │  │ Node 3   │              │
│  │ Metrics  │  │ Metrics  │  │ Metrics  │              │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘              │
│       │             │             │                     │
│       └─────────────┴─────────────┘                     │
│                    Prometheus                           │
│       ┌─────────────────────────────┐                   │
│       │      Grafana Dashboard      │                   │
│       └─────────────────────────────┘                   │
│       ┌─────────────────────────────┐                   │
│       │      Alertmanager           │                   │
│       └─────────────────────────────┘                   │
└─────────────────────────────────────────────────────────┘
```

---

## 2. 核心指标

### 2.1 系统指标

| 指标 | 类型 | 说明 |
|------|------|------|
| `node_uptime_seconds` | Counter | 节点运行时间 |
| `node_active_connections` | Gauge | 活跃连接数 |
| `node_memory_usage_bytes` | Gauge | 内存使用量 |
| `node_cpu_usage_percent` | Gauge | CPU 使用率 |

### 2.2 KV 缓存指标

| 指标 | 类型 | 说明 |
|------|------|------|
| `kv_cache_total_keys` | Gauge | KV 缓存总键数 |
| `kv_cache_hit_rate` | Gauge | KV 缓存命中率 |
| `kv_cache_l1_hit_rate` | Gauge | L1 缓存命中率 |
| `kv_cache_l2_hit_rate` | Gauge | L2 缓存命中率 |
| `kv_cache_l3_hit_rate` | Gauge | L3 缓存命中率 |
| `kv_cache_size_bytes` | Gauge | KV 缓存大小 |

### 2.3 推理指标

| 指标 | 类型 | 说明 |
|------|------|------|
| `inference_requests_total` | Counter | 推理请求总数 |
| `inference_latency_seconds` | Histogram | 推理延迟 |
| `inference_success_total` | Counter | 成功推理数 |
| `inference_failure_total` | Counter | 失败推理数 |

### 2.4 共识指标

| 指标 | 类型 | 说明 |
|------|------|------|
| `consensus_rounds_total` | Counter | 共识轮次总数 |
| `consensus_duration_seconds` | Histogram | 共识耗时 |
| `consensus_agreement_ratio` | Gauge | 共识一致率 |
| `active_validators` | Gauge | 活跃验证者数 |

---

## 3. Prometheus 配置

### 3.1 prometheus.yml

```yaml
# prometheus.yml

global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  - "alerts.yml"

scrape_configs:
  - job_name: 'blockchain'
    static_configs:
      - targets: ['localhost:3000']
        labels:
          node: 'node_1'
      - targets: ['localhost:3001']
        labels:
          node: 'node_2'
      - targets: ['localhost:3002']
        labels:
          node: 'node_3'

  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']
```

### 3.2 启动 Prometheus

```bash
# 使用 Docker
docker run -d \
  --name prometheus \
  -p 9090:9090 \
  -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml \
  prom/prometheus

# 访问 http://localhost:9090
```

---

## 4. Grafana 配置

### 4.1 添加数据源

```bash
# 启动 Grafana
docker run -d \
  --name grafana \
  -p 3001:3000 \
  grafana/grafana

# 访问 http://localhost:3001
# 默认账号：admin / admin
```

### 4.2 导入 Dashboard

导入 JSON 配置文件：

```json
{
  "dashboard": {
    "title": "KV Cache System Overview",
    "panels": [
      {
        "title": "KV Cache Hit Rate",
        "targets": [
          {
            "expr": "kv_cache_hit_rate",
            "legendFormat": "{{node}}"
          }
        ]
      },
      {
        "title": "Inference Latency",
        "targets": [
          {
            "expr": "histogram_quantile(0.99, inference_latency_seconds_bucket)",
            "legendFormat": "P99"
          }
        ]
      }
    ]
  }
}
```

---

## 5. 告警配置

### 5.1 alerts.yml

```yaml
# alerts.yml

groups:
  - name: blockchain_alerts
    rules:
      - alert: NodeDown
        expr: up == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Node {{ $labels.node }} is down"
          description: "Node {{ $labels.node }} has been down for more than 1 minute."

      - alert: HighMemoryUsage
        expr: node_memory_usage_bytes / 1073741824 > 8
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High memory usage on {{ $labels.node }}"
          description: "Memory usage is above 8GB for more than 5 minutes."

      - alert: LowCacheHitRate
        expr: kv_cache_hit_rate < 0.5
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Low cache hit rate on {{ $labels.node }}"
          description: "Cache hit rate is below 50% for more than 10 minutes."

      - alert: HighInferenceLatency
        expr: histogram_quantile(0.99, inference_latency_seconds_bucket) > 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High inference latency on {{ $labels.node }}"
          description: "P99 inference latency is above 1s for more than 5 minutes."

      - alert: ConsensusFailure
        expr: consensus_agreement_ratio < 0.67
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Consensus failure on {{ $labels.node }}"
          description: "Consensus agreement ratio is below 67%."
```

### 5.2 Alertmanager 配置

```yaml
# alertmanager.yml

global:
  smtp_smarthost: 'smtp.example.com:587'
  smtp_from: 'alertmanager@example.com'

route:
  group_by: ['alertname', 'severity']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 1h
  receiver: 'email-notifications'

  routes:
    - match:
        severity: critical
      receiver: 'pagerduty'
    - match:
        severity: warning
      receiver: 'email-notifications'

receivers:
  - name: 'email-notifications'
    email_configs:
      - to: 'team@example.com'

  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: '<your-pagerduty-key>'
```

---

## 6. 日志聚合

### 6.1 Loki 配置

```yaml
# loki.yml

auth_enabled: false

server:
  http_listen_port: 3100

common:
  path_prefix: /loki
  replication_factor: 1

schema_config:
  configs:
    - from: 2020-10-24
      store: boltdb-shipper
      object_store: filesystem
      schema: v11
      index:
        prefix: index_
        period: 24h

storage_config:
  boltdb_shipper:
    active_index_directory: /loki/index
    cache_location: /loki/cache
  filesystem:
    directory: /loki/chunks
```

### 6.2 Promtail 配置

```yaml
# promtail.yml

server:
  http_listen_port: 9080

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://localhost:3100/loki/api/v1/push

scrape_configs:
  - job_name: blockchain
    static_configs:
      - targets:
          - localhost
        labels:
          job: blockchain
          __path__: /opt/blockchain/logs/*.log
```

---

## 7. 监控脚本

### 7.1 健康检查脚本

```bash
#!/bin/bash
# health_check.sh

NODES=("localhost:3000" "localhost:3001" "localhost:3002")

for node in "${NODES[@]}"; do
    response=$(curl -s -o /dev/null -w "%{http_code}" http://$node/health)
    
    if [ "$response" != "200" ]; then
        echo "Node $node is down (HTTP $response)"
        # 发送告警
        curl -X POST http://alertmanager:9093/api/v1/alerts \
            -H "Content-Type: application/json" \
            -d "[{
                \"labels\": {
                    \"alertname\": \"NodeDown\",
                    \"node\": \"$node\",
                    \"severity\": \"critical\"
                }
            }]"
    fi
done
```

### 7.2 指标收集脚本

```bash
#!/bin/bash
# metrics_collector.sh

# 收集系统指标
echo "node_memory_usage_bytes $(free -b | awk '/^Mem:/{print $3}')"

# 收集 KV 缓存指标
hit_rate=$(curl -s http://localhost:3000/metrics | grep kv_cache_hit_rate)
echo "kv_cache_hit_rate $hit_rate"

# 收集推理指标
latency=$(curl -s http://localhost:3000/metrics | grep inference_latency_p99)
echo "inference_latency_seconds $latency"
```

---

## 8. Dashboard 示例

### 8.1 系统概览

```
┌─────────────────────────────────────────────────────────┐
│              KV Cache System Dashboard                   │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Node Status      KV Cache Hit Rate    Inference Latency │
│  ● Node 1: Up     85%                  P99: 120ms       │
│  ● Node 2: Up     82%                  P99: 115ms       │
│  ● Node 3: Up     88%                  P99: 110ms       │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │         KV Cache Hit Rate (Last 1 Hour)          │   │
│  │  [图表：三条曲线，分别代表三个节点]                  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │        Inference Requests (Last 1 Hour)          │   │
│  │  [柱状图：每分钟请求数]                             │   │
│  └──────────────────────────────────────────────────┘   │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

---

## 9. 相关文档

- [部署指南](01-deployment.md) - 单节点、多节点部署
- [故障排查](03-troubleshooting.md) - 常见问题、排查流程
- [性能调优](../../06-KV_CACHE_OPTIMIZATION.md) - 性能指标、优化建议

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
