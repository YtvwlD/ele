# ele

ele spawns elevated processes. To make this work, there are two pieces:

## ele

`ele` is a command line application. You can call it the way you might expect:

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
as root (eled) which spawns the elevated processes and passes over the file
descriptors of the applications. Authentication is done via polkit,
dbus is used as the transport. This design is inspired by
[su on LineageOS](https://github.com/LineageOS/android_system_extras_su).

Why is this any better? Isn't this just more complicated?

`su` and `sudo` being setuid means that the authentication prompt itself is
running as root. This makes them (a bit) vulnerable against attacks because
the environment can't really be controlled.
See [CVE-2023-6246](https://www.qualys.com/2024/01/30/cve-2023-6246/syslog.txt)
for a recent vulnerability in this fashion.

[`sudo-rs`](https://github.com/memorysafety/sudo-rs) is an improvement because
it's (hopefully) not affected by such memory corruption shenanigans,
but still, setuid itself poses some risk.

[polkit](https://github.com/polkit-org/polkit) provides fine-grained access
control and many setuid binaries can probably be replaced with a combination of
client and daemon, connected via dbus and polkit.
Interestingly, `pkexec` just uses polkit for authentication -- the binary itself
is setuid.

(Please don't use it, though, as this is currently just a proof of concept.)
