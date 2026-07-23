# Client configuration

You can generate a configuration file with
```shell
pipeline client config client.toml
```

> [!IMPORTANT]
> Relative paths in the configuration file are resolved with respect to the
> configuration file location.

## Client name

This is a convenience to identify more easily the provenance of files on the
server side. This can also be accessed in `processing` commands on the server
side via the `{client_name}` placeholder.
```toml
name = "client_name"
```

## Sending files to the server

`copy_to_server` defines a command to copy a file to the server for processing.
This is a list of strings representing the command and its arguments. The
following placeholders are replaced at runtime:
- `{client_path}` is the absolute path of the file on the client, see
  the `[watching]` section for how files are discovered;
- `{server_filename}` is the name of the file as expected by the pipeline.

> [!IMPORTANT]
> This `copy_to_server` command must result in the file being copied to the
> `incoming_directory` (see the server configuration) with file name
> `{server_filename}`.

For example:
```toml
copy_to_server = [
    "cp",
    "{client_path}",
    "./server/buckets/{server_filename}",
]
```

> [!TIP]
> If the server filesystem is mounted locally, you can ask pipeline to copy the
> file instead of relying on an external process. For instance, the copy in the
> example command can be more efficiently achieved with
> ```toml
> copy_to_server = { destination = "./server/buckets" }
> ```
>
> If the server and the client operate on the same filesystem, you can ask
> pipeline to merely rename the file for better performance:
> ```toml
> copy_to_server = { move_in_same_fs_to = "./server/buckets" }
> ```

## Connection to the server

This is done in a dedicated section.

### Direct connection

If the socket is directly available, you can specify the address:
```toml
[server]
address = "127.0.0.1:47890"
```

### SSH tunnelling

`pipeline` also support connecting to the server via an SSH tunnel.

```toml
[server]
ssh_host = "192.168.0.1"
ssh_port = 22
ssh_auth = { method = "none", user = "user" }
keepalive_every_secs = 60
server_addr_from_host = "127.0.0.1"
server_port_from_host = 47890
accepted_ssh_keys = []
```

- `ssh_host` is the address of the ssh host for tunnelling.
- `ssh_port` is the port used by the host for SSH connections.
- `ssh_auth` specifies how SSH authentication should happen. Supported options
  are:
    - no authentication (e.g. if otherwise managed by network interface)
      ```toml
      ssh_auth = { method = "none", user = "user" }
      ```
    - ask a password to user (requires keyboard interaction)
      ```toml
      ssh_auth = { method = "password", user = "user" }
      ```
    - use a key via the openssh agent at `SSH_AUTH_SOCK`
      ```toml
      ssh_auth = { method = "key", user = "user", public_key = "path/to/key.pub" }
      ```
      You can also specify the SSH socket to use with `agent = 'path/to/socket'`.
- `keepalive_every_secs` sends a keepalive request if no communication occurs
  for this duration in seconds.
- `server_addr_from_host` and `server_port_from_host` specify the network
  socket of the pipeline server as seen from the SSH host.
- `accepted_ssh_keys` is a list of accepted keys from the SSH host (in OpenSSH
  format).

## Watching for files to process

The pipeline client continuously watches a directory to find new files to process.
This is done in the `watching` section. The high level options are the following:

```toml
[watching]
directory = "./client"
refresh_every_secs = 5
max_concurrent_hashes = 3
heartbeat_every_refreshes = 10
```
- `directory` is the path of the directory being watched.
- `refresh_every_secs` sets how often the client should look for new files in
  the watched directory, in seconds.
- `max_concurrent_hashes` sets the maximum concurrent computations of file
  hashes.
- `heartbeat_every_refreshes` sets the number of refreshes before logging out a
  heartbeat detailing how many files have been found since the last heartbeat.
  Set to 0 to disable heartbeat.

### Watching groups

The client can sort files found in the watched directory into different groups
(e.g. to request a different [processing command to the
server](server.md#processing-groups)).

Defining a group is done as follow:
```toml
[[watching.groups]]
filters = { extension = "dat" }
processing = "main"
last_modif_secs = 10
full_hash = true
```

As many group as necessary can be defined by appending a `[[watching.groups]]`
section to the configuration file.

For each file in the watched directory, groups are considered in the order they
are defined in the configuration file. A file will end up in the first group
whose `filters` it passes (even if `last_modif_secs` have not elapsed yet).

- `filters` is the set of filters applying to this group. Available filters are:
  - `extension`, checking the file extension matches the given name
  - `min_depth` and `max_depth`, checking the depth of the file relative to
    the watched directory (note, the directory itself is depth 0, all files
    are therefore of depth 1 at least). Both values are inclusive.
- `processing` defines which processing group should be run for these files
  (see server configuration).
- `last_modif_secs` defines how long ago the last modification should be (in
  seconds) for the file to be considered ready for processing. This is useful
  if the files are created and enriched in a non-atomic manner by some other
  process.
- `full_hash` specifies whether to use full hashes (of the entire file
  contents), or shallow hashes. It is recommended to use full hashes when
  possible for more robust data integrity check. Shallow hashes should be
  reserved for when the pipeline has to process large files for which
  computing the full hash is too demanding.
- `keep_on_client` (`false` if unspecified) requests the pipeline not to delete
  the files that are sent for processing on the client side. This is meant for
  workflows where those files are still being used on the client side by some
  other process. This option should be used parsimoniously as files might
  accumulate in the watched directory if not otherwise deleted, leading to a
  more and more costly watching-loop with time.
