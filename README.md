# Rust Health Endpoint

This is a repo for use in personal projects.
It allows running a health endpoint for use in containerized setting (docker, kube etc.) with the healthiness report exposed as a tokio broadcast message.

```rust
let health_monitor = HealthMonitor::new(Duration::from_millis(5_000));
let publisher = health_monitor.get_publisher();

// Run code etc.

futures::join(
    health_monitor.run(),
    // Your task.
);
```

Since it's a personal use it's prone to changes and there is no guarantees on compatibility.