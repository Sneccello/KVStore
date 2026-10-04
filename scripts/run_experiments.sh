#!/usr/bin/env bash
set -e

echo "=== Building mem_bench in release mode ==="
cargo build --release --bin mem_bench

mkdir -p experiment_data

configs=(
    "configs/exp_no_wal_read.yaml"
    "configs/exp_no_wal_write.yaml"
    "configs/exp_wal_read.yaml"
    "configs/exp_wal_write.yaml"
)

for cfg in "${configs[@]}"; do
    echo "=================================================="
    echo "Starting experiment with config: $cfg"
    echo "=================================================="
    # Clean up DB & WAL files before each fresh run
    rm -f kv.db kv.db.wal
    ./target/release/mem_bench "$cfg"
    echo "Completed: $cfg"
    echo ""
done

echo "=== All experiments completed successfully! ==="
