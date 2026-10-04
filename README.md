A B+tree project I used as an intro to Rust.

Operations:
- Put key-value
- Get key
- Delete key


How to run:
```
cargo run --bin server
```

```
curl -X PUT http://127.0.0.1:3000/kv/my_key -d "my_value"   #-> 200 OK
curl -X GET 127.0.0.1:3000/kv/mykey                         #-> 200 OK("my_value")
curl -X DELETE  127.0.0.1:3000/kv/mykey                     #-> 200 OK
curl -X GET 127.0.0.1:3000/kv/mykey                         #-> 404 NotFound("my_key")
```

About:

The tree itself is fully thread-safe, uses RWLocks on each node for multithreading.
It provides durability with a Write-ahead log (WAL) which persists the put and deleted data before returning from the operation to the client. WAL entry writes are batched to support higher traffic. 

Below the plot shows the saturation points of Read/Write-heavy workloads on the tree with/without the WAL.

<img width="1077" height="630" alt="Unknown" src="https://github.com/user-attachments/assets/21850619-b995-4461-ab63-8c2a23895f97" />

Future improvements:
- Currently each threads reads the root page by acquiring the RWLock of the root node. It should be ensured that the root node is always PageId=1 so this is not necessary
- Currently the number of workers is set high at 1024. This results in higher write throughput via more efficient WAL batching but results in lock contention for workloads that do not use WAL as much.
- Currently the WAL is written to disk but can grow infinitely. Changes should be persisted to disk so we can throw away parts of the WAL that refer to data already persisted.
- WAL reading back / log replaying is not implemented.
- PageManager memory is not limited currently. Eviction policy should be implemented
