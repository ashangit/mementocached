* ensure all tests work
* review protocol and error in request/reply
* update logging and error mgt => release 0.1.0

* add metrics => release 0.1.1

* see if possible to merge socket and db workers.
Idea is having a one thread per core reading from the socket and managing is set of db data.
If the thread is not in charge of the key it will still send req to appropriate thread (as already performed).
Need to validate if it has a good or bad impact on perf
Perhaps having the socket in the core and after each accept send the stream to a topic that is read by all DB threads
* Add assigning thread to a dedicated hyper thread => also see if better perf

* update worker to manage 4k of hashmap spread on all workers
* manage on disk data => release as 0.2.0
* multi node
  * define archi and mechanism
  * split in multiple step (replication, recover, spread...)
