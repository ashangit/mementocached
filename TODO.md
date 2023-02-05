* review protocol and error in request/reply

* add metrics

* Add assigning thread to a dedicated hyper thread => also see if better perf

* look at https://tokio.rs/blog/2023-01-03-announcing-turmoil for testing

* update worker to manage 4k of hashmap spread on all workers
* manage on disk data
* multi node
  * define archi and mechanism
  * split in multiple step (replication, recover, spread...)
