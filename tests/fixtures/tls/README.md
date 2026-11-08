# Loopback TLS fixture

Synthetic CA, localhost certificate and public test server key for CLI integration
tests. The CA signing key was discarded. These files identify only the test
server (`localhost` / `127.0.0.1`) and contain no instance or PAT credentials.

The test checks untrusted rejection, explicit PEM trust and shared TLS behavior
for PAT diagnostics, REST writes and cookie-authenticated Web reads.
