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
pipeline print-config server server.toml
# edit `server.toml` as required
pipeline server server.toml
```

Similarly on the clients:

```shell
pipeline print-config client [--ssh-tunnel] client.toml
# edit `client.toml` as required
pipeline client client.toml
```

The configuration files produced by the `print-config` command contain comments
explaining each configuration option. The `--ssh-tunnel` option produces a
configuration file that uses SSH tunnelling to connect to the server.

Both the client and server processes are designed as long running processes
with low CPU- and memory-footprints. They are intended to run as daemons to
process a large number of files over hours, days, or longer durations.

You can set the `PIPELINE_LOG` environment variable to change the verbosity of
logs. Accepted values in order of decreasing verbosity are:

- `debug`: the most verbose level;
- `info`: the default level;
- `warn`: only show warnings;
- `off`: disable logging.
