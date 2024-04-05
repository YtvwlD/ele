# ele

ele spawns elevated processes. To make this work, there are two pieces:

## ele

*ele* is a command line application. You can call it the way you might expect:

```sh
$ ele id
uid=0(root) gid=0(root) groups=0(root)
```

At least for non-interactive applications.

For applications that need access to the terminal (like a shell), use `-i`:

```sh
$ ele --interactive bash
root@localhost:~/dev/rust/ele#
```

## eled

This is the daemon that actually spawns the processes. Currently, it has to be
running (you can archieve this with a systemd unit) to be able to react to
requests; dbus activation is a work in progress.

## Why?

`sudo` and `su` spawn elevated processes without needing a long-running
system-wide daemon. They also handle the terminal way better.

So why use ele?

ele doesn't need to be setuid root to work. Instead, there's a daemon running
as root (eled) which spawns the elevated processes. Authentication is done via
polkit, dbus is used as the transport.
This design is inspired by [su on LineageOS](https://github.com/LineageOS/android_system_extras_su).

(Please don't use it, though, as this is currently just a proof of concept.)
