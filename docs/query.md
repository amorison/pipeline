# Queries

The `pipeline query` subcommand performs queries to the pipeline server to know
about its status or mark tasks with a given status. Its configuration file can
be generated with

```shell
pipeline query config query.toml
```

Note however that all that's needed is the server address, be it from the
[server side](server.md#server-address) or the [client
side](client.md#connection-to-the-server). In practice, this means you can use
a server or client configuration file instead of a dedicated `query.toml`
