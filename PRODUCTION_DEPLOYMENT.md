# Production Deployment Guide for qml

This guide covers deploying qml in production environments with multiple storage backends, proper monitoring, and scaling strategies.

## 📋 **Table of Contents**

- [Quick Production Checklist](#quick-production-checklist)
- [PostgreSQL Production Setup](#postgresql-production-setup)
- [Redis Cluster Configuration](#redis-cluster-configuration)
- [Docker Deployment](#docker-deployment)
- [Kubernetes Deployment](#kubernetes-deployment)
- [Monitoring & Observability](#monitoring--observability)
- [Performance Tuning](#performance-tuning)
- [Security Considerations](#security-considerations)
- [Scaling Strategies](#scaling-strategies)
- [Troubleshooting](#troubleshooting)

## 🚀 **Quick Production Checklist**

### ✅ **Essential Steps**

- [ ] PostgreSQL or Redis cluster configured
- [ ] Connection pooling optimized (50+ connections for PostgreSQL)
- [ ] Database migrations applied (`auto_migrate: true`)
- [ ] Environment variables secured (no hardcoded credentials)
- [ ] Worker count tuned (1-2x CPU cores)
- [ ] Job timeouts configured (prevent runaway jobs)
- [ ] Queue prioritization implemented
- [ ] Monitoring and metrics enabled
- [ ] Graceful shutdown handling
- [ ] Resource limits set (memory, CPU)

### 📊 **Performance Targets**

- **PostgreSQL**: 5,000+ jobs/second
- **Redis**: 10,000+ jobs/second
- **Memory**: 50,000+ jobs/second (dev only)
- **Latency**: < 100ms job pickup time
- **Availability**: 99.9% uptime target

## 🐘 **PostgreSQL Production Setup**

### **Database Initialization**

```sql
-- Create database and user
CREATE DATABASE hangfire_production;
CREATE USER hangfire_app WITH PASSWORD 'your_secure_password_here';

-- Grant permissions
GRANT ALL PRIVILEGES ON DATABASE hangfire_production TO hangfire_app;
\c hangfire_production
GRANT ALL ON SCHEMA public TO hangfire_app;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO hangfire_app;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO hangfire_app;
```

### **Production Configuration**

```rust
use qml::{PostgresConfig, StorageInstance};
use std::time::Duration;

let config = PostgresConfig::new()
    .with_database_url(&std::env::var("DATABASE_URL")?)
    .with_max_connections(50)           // Scale based on concurrent workers
    .with_min_connections(5)            // Keep warm connections
    .with_connect_timeout(Duration::from_secs(30))
    .with_idle_timeout(Some(Duration::from_secs(300)))    // 5 minutes
    .with_max_lifetime(Some(Duration::from_secs(1800)))   // 30 minutes
    .with_auto_migrate(true)            // Apply migrations on startup
    .with_statement_timeout(Duration::from_secs(60));     // Prevent long queries

let storage = StorageInstance::postgres(config).await?;
```

### **Database Optimization**

```sql
-- Performance tuning for PostgreSQL
-- Add to postgresql.conf

-- Connection settings
max_connections = 200
shared_buffers = 256MB
effective_cache_size = 1GB

-- Performance settings
random_page_cost = 1.1
seq_page_cost = 1.0
default_statistics_target = 100

-- Write-ahead logging
wal_buffers = 16MB
checkpoint_completion_target = 0.9
wal_compression = on

-- Index optimization
maintenance_work_mem = 256MB
work_mem = 4MB
```

### **Connection Pooling with PgBouncer**

```ini
# pgbouncer.ini
[databases]
hangfire_production = host=localhost port=5432 dbname=hangfire_production

[pgbouncer]
pool_mode = transaction
listen_port = 6543
max_client_conn = 100
default_pool_size = 20
server_reset_query = DISCARD ALL
```

### **Environment Variables**

```bash
# Production environment variables
export DATABASE_URL="postgresql://hangfire_app:password@pgbouncer:6543/hangfire_production"
export HANGFIRE_WORKERS=20
export HANGFIRE_QUEUES="critical,normal,bulk"
export RUST_LOG=info
export HANGFIRE_JOB_TIMEOUT=300
```

## 🔴 **Redis Cluster Configuration**

### **Redis Cluster Setup**

```bash
# Redis cluster configuration
# redis.conf for each node

port 7000
cluster-enabled yes
cluster-config-file nodes.conf
cluster-node-timeout 5000
appendonly yes
appendfilename "appendonly.aof"

# Memory optimization
maxmemory 2gb
maxmemory-policy allkeys-lru

# Persistence
save 900 1
save 300 10
save 60 10000
```

### **Production Redis Configuration**

```rust
use qml::{RedisConfig, StorageInstance};
use std::time::Duration;

let config = RedisConfig::new()
    .with_url("redis://redis-cluster:6379")
    .with_pool_size(30)                 // Scale with worker count
    .with_connection_timeout(Duration::from_secs(10))
    .with_command_timeout(Duration::from_secs(5))
    .with_key_prefix("hangfire:prod")
    .with_database(Some(1))             // Use dedicated database
    .with_password(Some(std::env::var("REDIS_PASSWORD")?))
    .with_completed_job_ttl(Some(Duration::from_secs(86400)))  // 24h retention
    .with_failed_job_ttl(Some(Duration::from_secs(604800)));   // 7 days retention

let storage = StorageInstance::redis(config).await?;
```

### **Redis Sentinel for High Availability**

```bash
# sentinel.conf
port 26379
sentinel monitor mymaster redis-master 6379 2
sentinel down-after-milliseconds mymaster 5000
sentinel parallel-syncs mymaster 1
sentinel failover-timeout mymaster 10000
```

## 🐳 **Docker Deployment**

### **Multi-Stage Dockerfile**

```dockerfile
# Dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

RUN cargo build --release --features postgres

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/hangfire-app .
COPY --from=builder /app/migrations ./migrations

ENV RUST_LOG=info
EXPOSE 8080

USER 1000:1000

CMD ["./hangfire-app"]
```

### **Production Docker Compose**

```yaml
# docker-compose.prod.yml
version: "3.8"

services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_DB: hangfire_production
      POSTGRES_USER: hangfire_app
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./init.sql:/docker-entrypoint-initdb.d/init.sql
    command: |
      postgres
      -c shared_buffers=256MB
      -c max_connections=200
      -c effective_cache_size=1GB
    deploy:
      resources:
        limits:
          memory: 1G
          cpus: "1.0"
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes --maxmemory 512mb --maxmemory-policy allkeys-lru
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data
    deploy:
      resources:
        limits:
          memory: 512M
          cpus: "0.5"
    restart: unless-stopped

  hangfire-app:
    build: .
    environment:
      DATABASE_URL: postgresql://hangfire_app:${POSTGRES_PASSWORD}@postgres:5432/hangfire_production
      HANGFIRE_WORKERS: 10
      HANGFIRE_QUEUES: critical,normal,bulk
      RUST_LOG: info
    depends_on:
      - postgres
    ports:
      - "8080:8080"
    deploy:
      replicas: 2
      resources:
        limits:
          memory: 512M
          cpus: "1.0"
        reservations:
          memory: 256M
          cpus: "0.5"
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  hangfire-dashboard:
    build: .
    command: ["./hangfire-app", "--dashboard-only"]
    environment:
      DATABASE_URL: postgresql://hangfire_app:${POSTGRES_PASSWORD}@postgres:5432/hangfire_production
    depends_on:
      - postgres
    ports:
      - "8081:8080"
    deploy:
      resources:
        limits:
          memory: 256M
          cpus: "0.5"
    restart: unless-stopped

volumes:
  postgres_data:
  redis_data:

networks:
  default:
    driver: bridge
```

### **Environment Configuration**

```bash
# .env.prod
POSTGRES_PASSWORD=your_ultra_secure_password_here
COMPOSE_PROJECT_NAME=hangfire-prod
COMPOSE_FILE=docker-compose.prod.yml
```

## ☸️ **Kubernetes Deployment**

### **ConfigMap and Secrets**

```yaml
# hangfire-config.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: hangfire-config
  namespace: hangfire
data:
  HANGFIRE_WORKERS: "20"
  HANGFIRE_QUEUES: "critical,normal,bulk"
  RUST_LOG: "info"
  HANGFIRE_JOB_TIMEOUT: "300"
  HANGFIRE_POLLING_INTERVAL: "1"

---
apiVersion: v1
kind: Secret
metadata:
  name: hangfire-secrets
  namespace: hangfire
type: Opaque
data:
  database-url: cG9zdGdyZXNxbDovL2hhbmdmaXJlX2FwcDpwYXNzd29yZEBwb3N0Z3Jlcy1zZXJ2aWNlOjU0MzIvaGFuZ2ZpcmVfcHJvZHVjdGlvbg== # base64 encoded
  redis-password: cmVkaXNfcGFzc3dvcmQ= # base64 encoded
```

### **PostgreSQL StatefulSet**

```yaml
# postgres-statefulset.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres
  namespace: hangfire
spec:
  serviceName: postgres-service
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
        - name: postgres
          image: postgres:15
          ports:
            - containerPort: 5432
          env:
            - name: POSTGRES_DB
              value: hangfire_production
            - name: POSTGRES_USER
              value: hangfire_app
            - name: POSTGRES_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: postgres-secret
                  key: password
          args:
            - "postgres"
            - "-c"
            - "shared_buffers=256MB"
            - "-c"
            - "max_connections=200"
            - "-c"
            - "effective_cache_size=1GB"
          volumeMounts:
            - name: postgres-storage
              mountPath: /var/lib/postgresql/data
          resources:
            requests:
              memory: "512Mi"
              cpu: "500m"
            limits:
              memory: "2Gi"
              cpu: "2000m"
          livenessProbe:
            exec:
              command:
                - pg_isready
                - -U
                - hangfire_app
            initialDelaySeconds: 30
            timeoutSeconds: 5
          readinessProbe:
            exec:
              command:
                - pg_isready
                - -U
                - hangfire_app
            initialDelaySeconds: 5
            timeoutSeconds: 1
  volumeClaimTemplates:
    - metadata:
        name: postgres-storage
      spec:
        accessModes: ["ReadWriteOnce"]
        resources:
          requests:
            storage: 20Gi
        storageClassName: fast-ssd

---
apiVersion: v1
kind: Service
metadata:
  name: postgres-service
  namespace: hangfire
spec:
  selector:
    app: postgres
  ports:
    - port: 5432
      targetPort: 5432
  clusterIP: None
```

### **QML Job Processing Deployment**

```yaml
# hangfire-workers.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hangfire-workers
  namespace: hangfire
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app: hangfire-workers
  template:
    metadata:
      labels:
        app: hangfire-workers
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      containers:
        - name: hangfire
          image: your-registry/hangfire-app:latest
          ports:
            - containerPort: 8080
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: hangfire-secrets
                  key: database-url
          envFrom:
            - configMapRef:
                name: hangfire-config
          resources:
            requests:
              memory: "256Mi"
              cpu: "250m"
            limits:
              memory: "1Gi"
              cpu: "1000m"
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 30
            periodSeconds: 30
            timeoutSeconds: 5
            failureThreshold: 3
          readinessProbe:
            httpGet:
              path: /ready
              port: 8080
            initialDelaySeconds: 10
            periodSeconds: 5
            timeoutSeconds: 3
            failureThreshold: 2
          lifecycle:
            preStop:
              exec:
                command: ["/bin/sh", "-c", "sleep 15"] # Graceful shutdown
      terminationGracePeriodSeconds: 30
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
            - weight: 100
              podAffinityTerm:
                labelSelector:
                  matchExpressions:
                    - key: app
                      operator: In
                      values:
                        - hangfire-workers
                topologyKey: kubernetes.io/hostname
```

### **QML Dashboard Service**

```yaml
# hangfire-dashboard.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hangfire-dashboard
  namespace: hangfire
spec:
  replicas: 2
  selector:
    matchLabels:
      app: hangfire-dashboard
  template:
    metadata:
      labels:
        app: hangfire-dashboard
    spec:
      containers:
        - name: dashboard
          image: your-registry/hangfire-app:latest
          command: ["./hangfire-app", "--dashboard-only"]
          ports:
            - containerPort: 8080
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: hangfire-secrets
                  key: database-url
          resources:
            requests:
              memory: "128Mi"
              cpu: "100m"
            limits:
              memory: "256Mi"
              cpu: "500m"

---
apiVersion: v1
kind: Service
metadata:
  name: hangfire-dashboard-service
  namespace: hangfire
spec:
  selector:
    app: hangfire-dashboard
  ports:
    - port: 80
      targetPort: 8080
  type: ClusterIP

---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: hangfire-dashboard-ingress
  namespace: hangfire
  annotations:
    kubernetes.io/ingress.class: nginx
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/auth-basic: "Authentication Required"
    nginx.ingress.kubernetes.io/auth-secret: hangfire-auth
spec:
  tls:
    - hosts:
        - hangfire.yourcompany.com
      secretName: hangfire-dashboard-tls
  rules:
    - host: hangfire.yourcompany.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: hangfire-dashboard-service
                port:
                  number: 80
```

### **Horizontal Pod Autoscaler**

```yaml
# hangfire-hpa.yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: hangfire-workers-hpa
  namespace: hangfire
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: hangfire-workers
  minReplicas: 3
  maxReplicas: 20
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
    - type: Resource
      resource:
        name: memory
        target:
          type: Utilization
          averageUtilization: 80
  behavior:
    scaleUp:
      stabilizationWindowSeconds: 60
      policies:
        - type: Percent
          value: 100
          periodSeconds: 15
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
        - type: Percent
          value: 10
          periodSeconds: 60
```

## 📊 **Monitoring & Observability**

### **Prometheus Metrics**

```rust
// Add to your main application
use prometheus::{Counter, Gauge, Histogram, Registry};

struct QMLMetrics {
    jobs_processed: Counter,
    jobs_failed: Counter,
    active_workers: Gauge,
    job_duration: Histogram,
    queue_size: Gauge,
}

impl QMLMetrics {
    fn new() -> Self {
        Self {
            jobs_processed: Counter::new("hangfire_jobs_processed_total", "Total processed jobs").unwrap(),
            jobs_failed: Counter::new("hangfire_jobs_failed_total", "Total failed jobs").unwrap(),
            active_workers: Gauge::new("hangfire_active_workers", "Number of active workers").unwrap(),
            job_duration: Histogram::with_opts(
                HistogramOpts::new("hangfire_job_duration_seconds", "Job execution duration")
                    .buckets(vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0])
            ).unwrap(),
            queue_size: Gauge::new("hangfire_queue_size", "Number of jobs in queue").unwrap(),
        }
    }
}
```

### **Grafana Dashboard Configuration**

```json
{
  "dashboard": {
    "title": "QML Rust - Job Processing",
    "panels": [
      {
        "title": "Job Processing Rate",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(hangfire_jobs_processed_total[5m])",
            "legendFormat": "Processed/sec"
          },
          {
            "expr": "rate(hangfire_jobs_failed_total[5m])",
            "legendFormat": "Failed/sec"
          }
        ]
      },
      {
        "title": "Queue Size",
        "type": "singlestat",
        "targets": [
          {
            "expr": "sum(hangfire_queue_size)",
            "legendFormat": "Total Jobs Queued"
          }
        ]
      },
      {
        "title": "Job Duration Percentiles",
        "type": "graph",
        "targets": [
          {
            "expr": "histogram_quantile(0.50, rate(hangfire_job_duration_seconds_bucket[5m]))",
            "legendFormat": "p50"
          },
          {
            "expr": "histogram_quantile(0.95, rate(hangfire_job_duration_seconds_bucket[5m]))",
            "legendFormat": "p95"
          },
          {
            "expr": "histogram_quantile(0.99, rate(hangfire_job_duration_seconds_bucket[5m]))",
            "legendFormat": "p99"
          }
        ]
      }
    ]
  }
}
```

### **Health Check Endpoints**

```rust
// Health check implementation
use axum::{routing::get, Router, Json};
use serde_json::json;

async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now(),
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn readiness_check(storage: Arc<dyn Storage>) -> Json<serde_json::Value> {
    // Check storage connectivity
    let storage_healthy = storage.get_job_counts().await.is_ok();

    Json(json!({
        "status": if storage_healthy { "ready" } else { "not_ready" },
        "checks": {
            "storage": storage_healthy
        }
    }))
}

fn create_health_router(storage: Arc<dyn Storage>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(move || readiness_check(storage)))
}
```

## 🚀 **Performance Tuning**

### **Worker Configuration**

```rust
use qml::ServerConfig;
use std::time::Duration;

// Production-optimized server configuration
let config = ServerConfig::new("production-server")
    .worker_count(num_cpus::get() * 2)  // 2x CPU cores
    .polling_interval(Duration::from_millis(500))  // Aggressive polling
    .job_timeout(Duration::from_secs(300))         // 5-minute timeout
    .queues(vec![
        "critical".to_string(),    // High priority
        "normal".to_string(),      // Normal priority
        "bulk".to_string(),        // Low priority
    ])
    .fetch_batch_size(5)           // Process multiple jobs
    .enable_scheduler(true);       // Handle scheduled jobs
```

### **Database Connection Tuning**

```rust
// PostgreSQL production settings
let config = PostgresConfig::new()
    .with_database_url(&database_url)
    .with_max_connections(50)      // Scale with worker count
    .with_min_connections(10)      // Keep warm connections
    .with_connect_timeout(Duration::from_secs(10))
    .with_idle_timeout(Some(Duration::from_secs(300)))
    .with_max_lifetime(Some(Duration::from_secs(1800)));
```

### **Queue Prioritization Strategy**

```rust
// Implement queue-specific workers for optimal performance
let critical_server = ServerConfig::new("critical-worker")
    .worker_count(5)
    .queues(vec!["critical".to_string()])
    .polling_interval(Duration::from_millis(100));  // Fast polling

let normal_server = ServerConfig::new("normal-worker")
    .worker_count(10)
    .queues(vec!["normal".to_string()])
    .polling_interval(Duration::from_secs(1));

let bulk_server = ServerConfig::new("bulk-worker")
    .worker_count(5)
    .queues(vec!["bulk".to_string()])
    .polling_interval(Duration::from_secs(5));      // Slower polling
```

## 🔒 **Security Considerations**

### **Database Security**

```bash
# Environment variables for secure configuration
export DATABASE_URL="postgresql://hangfire_app:${POSTGRES_PASSWORD}@postgres:5432/hangfire?sslmode=require"
export REDIS_URL="rediss://user:${REDIS_PASSWORD}@redis:6380/1"  # SSL enabled
```

### **Network Security**

```yaml
# Kubernetes Network Policy
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: hangfire-network-policy
  namespace: hangfire
spec:
  podSelector:
    matchLabels:
      app: hangfire-workers
  policyTypes:
    - Ingress
    - Egress
  ingress:
    - from:
        - podSelector:
            matchLabels:
              app: hangfire-dashboard
      ports:
        - protocol: TCP
          port: 8080
  egress:
    - to:
        - podSelector:
            matchLabels:
              app: postgres
      ports:
        - protocol: TCP
          port: 5432
```

### **Secret Management**

```bash
# Use external secret management
kubectl create secret generic hangfire-secrets \
  --from-literal=database-url="postgresql://..." \
  --from-literal=redis-password="..." \
  --dry-run=client -o yaml | kubectl apply -f -
```

## 📈 **Scaling Strategies**

### **Horizontal Scaling**

```yaml
# Scale based on queue depth
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: hangfire-queue-based-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: hangfire-workers
  minReplicas: 3
  maxReplicas: 50
  metrics:
    - type: External
      external:
        metric:
          name: hangfire_queue_size
        target:
          type: Value
          value: "100" # Scale when queue > 100 jobs
```

### **Vertical Scaling**

```yaml
# Vertical Pod Autoscaler
apiVersion: autoscaling.k8s.io/v1
kind: VerticalPodAutoscaler
metadata:
  name: hangfire-workers-vpa
spec:
  targetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: hangfire-workers
  updatePolicy:
    updateMode: "Auto"
  resourcePolicy:
    containerPolicies:
      - containerName: hangfire
        maxAllowed:
          memory: 2Gi
          cpu: 2000m
        minAllowed:
          memory: 128Mi
          cpu: 100m
```

### **Load Testing**

```rust
// Load testing script
use qml::{Job, Storage, MemoryStorage};
use tokio::time::{sleep, Duration};

async fn load_test() -> Result<(), Box<dyn std::error::Error>> {
    let storage = MemoryStorage::new();

    // Create 10,000 jobs
    let mut handles = Vec::new();
    for i in 0..10_000 {
        let storage = storage.clone();
        let handle = tokio::spawn(async move {
            let job = Job::new("load_test", vec![i.to_string()]);
            storage.enqueue(&job).await
        });
        handles.push(handle);
    }

    // Wait for all jobs to be enqueued
    for handle in handles {
        handle.await??;
    }

    println!("Enqueued 10,000 jobs successfully");
    Ok(())
}
```

## 🔧 **Troubleshooting**

### **Common Issues**

#### **High Memory Usage**

```bash
# Check memory usage
kubectl top pods -n hangfire

# Increase memory limits
kubectl patch deployment hangfire-workers -p '{"spec":{"template":{"spec":{"containers":[{"name":"hangfire","resources":{"limits":{"memory":"2Gi"}}}]}}}}'
```

#### **Database Connection Pool Exhaustion**

```rust
// Increase connection pool size
let config = PostgresConfig::new()
    .with_max_connections(100)  // Increase from 50
    .with_connect_timeout(Duration::from_secs(30));
```

#### **Job Processing Delays**

```bash
# Check queue sizes
curl -s http://hangfire-dashboard/api/stats | jq '.queue_sizes'

# Scale up workers
kubectl scale deployment hangfire-workers --replicas=10
```

### **Monitoring Commands**

```bash
# Check job processing metrics
kubectl exec -it hangfire-workers-xxx -- curl localhost:8080/metrics | grep hangfire

# View logs
kubectl logs -f deployment/hangfire-workers -n hangfire

# Check database queries
kubectl exec -it postgres-0 -- psql -U hangfire_app -d hangfire_production -c "
  SELECT query, state, query_start
  FROM pg_stat_activity
  WHERE datname = 'hangfire_production';
"
```

### **Performance Debugging**

```sql
-- Check slow queries
SELECT query, mean_time, calls, total_time
FROM pg_stat_statements
WHERE query LIKE '%hangfire%'
ORDER BY mean_time DESC
LIMIT 10;

-- Check index usage
SELECT schemaname, tablename, indexname, idx_scan, idx_tup_read, idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;
```

## 📚 **Additional Resources**

- [PostgreSQL Performance Tuning](https://wiki.postgresql.org/wiki/Performance_Optimization)
- [Redis Cluster Configuration](https://redis.io/docs/reference/cluster-spec/)
- [Kubernetes Best Practices](https://kubernetes.io/docs/concepts/cluster-administration/manage-deployment/)
- [Prometheus Monitoring](https://prometheus.io/docs/guides/go-application/)

---

**Next Steps**: Once deployed, monitor your qml installation using the provided Grafana dashboards and scale based on actual workload requirements.
