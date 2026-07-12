# Server configuration

You can generate a configuration file with

```shell
pipeline server config server.toml
```

> [!IMPORTANT]
> Relative paths in the configuration file are resolved with respect to the
> configuration file location.

## Incoming directory

The `incoming_directory` option sets the path on the pipeline server where
files to process will be expected to be copied by the client.

```toml
# in server configuration
incoming_directory = "./server/buckets"
```

The `copy_to_server` command on the client side **must** copy the files to
process to that directory, with the name `{server_filename}`. For example
with `rsync`:

```toml
# in client configuration
copy_to_server = [
    "rsync",
    "{client_path}",
    "server_host:path/to/server/buckets/{server_filename}",
]
```

> [!TIP]
> In practice, `{server_filename}` will be replaced by a hash distributed in
> subfolders, e.g. `2d/af/2daf63e8`. The length and algorithm for the hash as
> well as the number and names of subfolders are implementation details and
> should not be relied upon. If the server runs on unix, you can control the
> mode (as in permissions typically set via `chmod`) of the last subfolder
> (here `af`) via the `unix_mode` option, e.g.
> ```toml
> # in server configuration
> unix_mode = "0o770"
> ```
> to set `770` as mode.

## Server address

The server address is set in a dedicated section:

```toml
[server]
address = "127.0.0.1:47890"
```

Pick an appropriate subnet and port number so that the client(s) can access the
server. [See the corresponding client configuration](client.md#connection-to-the-server).

## Processing groups

A pipeline server can define an arbitrary set of "processing groups". Each
group defines what commands should be ran on each file to process.

For example:
```toml
[processing.main]
processing = [
    { create_directory = "./server/{client_relative_directory}" },
    [ "cp", "{server_path}", "./server/{client_relative_directory}/{client_file_stem}.out" ],
]
after_processing = { mark_as = "Done" }
```
defines the `main` processing group. You can define as many groups as you want.
To define a group with name `group_name`, append a section
`[processing.group_name]` to the configuration file.

> [!IMPORTANT]
> The `processing` option of each [watching group in the client
> configuration](client.md#watching-groups) **must** match a processing group
> in the server configuration. The connection to the server will be denied if
> the client configuration references non-existent processing groups.

### Processing command

`processing` defines the command(s) to apply to incoming files. This can be
either:
- an external command given as a list of strings representing the program
  name and its arguments;
- a `{ create_directory = "path" }` directive;
- a `{ delete_file = "path" }` directive;
- a `{ delete_directory = "path" }` directive;
- a list where each element is either of the previous;
- `"pass"` to not do anything.

The following placeholders are replaced at runtime:
- `{server_path}` is the path of the file on the server;
- `{client_name}` is the name of the client as defined in the client
  configuration file;
- `{client_relative_directory}` is the path to the file on the client,
  relative to the watched directory;
- `{client_file_stem}` is the file name on the client without its extension;
- `{client_file_name}` is the full file name on the client;
- `{hash}` is a unique hash identifying the file. Using it as part of the
  output filename of your processing command guarantees its uniqueness, so
  that processing different files does not overwrite output.

### After processing

What to do if the `processing` step was successful.

This can be either:
- `{ mark_as = "Done" }`: mark the job as completed;
- `{ mark_as = "ToPrune" }`: mark the job as completed and for pruning;
- `"pass"`: leave the job marked as being processed;
- `{ move_to_and_prune = "destination/path" }`: move the processed file to the
  given destination and prune it from database (the same substitutions as in
  `processing` are available). This uses an efficient rename if the
  destination is on the same filesystem as the `incoming_directory`, and is
  therefore more efficient than making a copy as part of `processing`.
  Note that this doesn't automatically create directories, make sure the
  processing command creates directories if necessary.

The `"pass"` option is intended for cases when the `processing` command
above doesn't actually perform the desired processing but instead schedules
it for execution (e.g. via a SLURM queue). In this scenario, the job has to
be manually marked as done, failed or to-prune by calling
```sh
pipeline query mark query.toml {hash} done|failed|to-prune
```

## Retrying failed tasks

You can control how often failed tasks are retried.

```toml
retry_tasks_every_secs = 60
```

will restart failed tasks every 60 seconds.

## Pruning `ToPrune` tasks

You can control how often tasks marked as `ToPrune` should be pruned. A pruned
task will be forgotten by the server and the incoming file deleted.

```toml
prune_every_secs = 120
```
will prune tasks every 120 seconds.

## Concurrency

You can control how many instances of demanding tasks are done at once:
```toml
[concurrency]
max_hashes = 3
max_processing = 8
```
- `max_hashes` controls the number of concurrent hash calculations;
- `max_processing` controls the number of concurrent processing tasks.

## Database

The server keeps track of incoming tasks and their status in an sqlite
database.
```toml
[database]
wal = false
```
Setting `wal` to `true` enables the WAL journaling mode. See
<https://www.sqlite.org/wal.html> for more information, in particular
regarding filesystem-related restrictions. Note that the pipeline server
holds an exclusive connection to the database, so it should in principle
always be fine to enable WAL. If `false`, the "truncate" mode is used.
