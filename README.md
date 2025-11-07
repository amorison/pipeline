pipeline
========

This is a command line tool to setup a processing pipeline.

The pipeline has two components:

- a `client`, watching a repository for files to process and
  sending them to a `server`;
- a `server`, waiting for files from any number of `client`s
  and starting a processing pipeline.

Both the client and server processes are designed as long running processes
with low CPU- and memory-footprints. They are intended to run as daemons to
process a large number of files over hours, days, or longer durations.

Quickstart
----------

You can create a configuration file and start the server with:

```shell
pipeline server config server.toml
# edit `server.toml` as required
pipeline server start server.toml
```

Similarly on the clients:

```shell
pipeline client config [--ssh-tunnel] client.toml
# edit `client.toml` as required
pipeline client start client.toml
```

The default configuration files generated as above contain comments
explaining each configuration option. The `--ssh-tunnel` option produces a
configuration file that uses SSH tunnelling to connect to the server.

You can set the `PIPELINE_LOG` environment variable to change the verbosity of
logs. Accepted values in order of decreasing verbosity are:

- `debug`: the most verbose level;
- `info`: the default level;
- `warn`: only show warnings;
- `off`: disable logging.

Client
------

The client configuration file contains a `watching` section where you can
setup the path of the directory where the client will find files to process.

The `copy_to_server` option in that configuration file specifies how files to
process should be sent to the server. You can either specify a local path where
files can be copied/moved to, or an arbitrary command to send files to a remote
location (e.g. via `rsync`). Note that `copy_to_server` should send the files
to process at the location specified by the `incoming_directory` option in the
configuration file of the server.

See comments in the generated configuration file for more details.

Server
------

The crux of the configuration on the server side is the `processing` option,
which describes the command(s) that should be ran for each file sent by
client(s). These processing commands are used to mark the processing of files
as done/failed depending on their exit status. If instead the processing step
merely schedules the actual processing (e.g. via Slurm) and you don't want the
scheduling command to wait for the job to complete, you can set the
`auto_status_update` option to `false`. This ignores completely the exit status
of the processing step. The processing can be marked as done/failed by calling
`pipeline server mark {hash} done|failed` in the directory of the server
configuration file (e.g. at the end of the scheduled Slurm job).

See comments in the generated configuration file for more details.

Acknowledgment
--------------

This tool was initially developed for processing of images produced by an
Apollo microscope.

We acknowledge the Scottish Centre for Macromolecular Imaging (SCMI) and Kako
Stapleton for assistance with cryo-EM experiments and access to
instrumentation, funded by the MRC (`MC_PC_17135`, `MC_UU_00034/7`,
`MR/X011879/1`) and SFC (`H17007`).

Funding for this project was provided by the MRC (`MC_UU_00034/7`,
`MR/X011879/1`).
