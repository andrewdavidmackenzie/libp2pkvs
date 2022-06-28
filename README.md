Libp2p KeyValue Store Example
=

This is the provided example for mDNS/kademlia Key-Value store from the libp2p-rust crate modified to 
learn how to use it and to reduce code side and to explore using it differently.

Published here just in case it can help someone else who is exploring libp2p.

P2P Key-Value Store
==
1. Using two terminal windows, start two instances. If you local network allows mDNS, they will automatically connect.
2. Type `PUT my-key my-value` in terminal one and hit return.
3. Type `GET my-key` in terminal two and hit return.
4. Close both with Ctrl-c.


You can also store provider records instead of key value records.
1. Using two terminal windows, start two instances. If you local network allows mDNS, they will automatically connect.
2. Type `PUT_PROVIDER my-key` in terminal one and hit return.
3. Type `GET_PROVIDERS my-key` in terminal two and hit return.
4. Close with Ctrl-c.