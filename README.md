pipeline
========

This is a command line tool to setup a processing pipeline.

The pipeline has two components:

- a `client`, watching a repository for files to process and
  sending them to a `server;
- a `server`, waiting for files from any number of `client`s
  and starting a processing pipeline.

Communication between the server and the clients occurs over TCP.

You can create a configuration file and start the server with:

```shell
pipeline print-config server > server.toml
# edit `server.toml` as required
pipeline server server.toml
```

Similarly on the clients:

```shell
pipeline print-config client > client.toml
# edit `client.toml` as required
pipeline client client.toml
```
