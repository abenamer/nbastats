---
type: Runbook
title: Reach the dashboard over Tailscale
description: Open the dashboard from your other devices through Tailscale Serve, over HTTPS, with no port open to the internet.
tags: [tailscale, network, runbook]
generated: { by: "human:abenamer", at: "2026-11-07T13:50:00Z" }
sources:
  - id: serve
    resource: https://tailscale.com/kb/1312/serve
    title: Tailscale Serve
---

# Purpose

The dashboard has no login. Do not publish port 3000 on the internet. Tailscale Serve lets the devices on your tailnet open the dashboard at `https://<machine>.<tailnet>.ts.net`. Tailscale ends the TLS connection on the computer that runs Compose and forwards each request to port 3000 on that computer. The port stays on loopback. Only the tailnet can reach the address; Tailscale Funnel, which opens an address to the internet, is not used.

# Before you start

* The application runs: `docker compose ps` shows both services `healthy`.
* The Tailscale client runs on the computer that runs Compose and is logged in to your tailnet. On WSL2, run `tailscaled` inside the Linux distribution (systemd on in `/etc/wsl.conf`): that node is the one that serves.
* HTTPS certificates are on for the tailnet (admin console, DNS, HTTPS Certificates). Tailscale gets the certificate for the machine's name and renews it. The project needs no certificate file.

# Procedure

1. Start the proxy in the background, so that it stays after a restart of Tailscale or of the computer:

       tailscale serve --bg 3000

   Tailscale prints the address, `https://<machine>.<tailnet>.ts.net/`. When the command needs root, let your user operate Tailscale once: `sudo tailscale set --operator=$USER`.

2. Add the machine's name to `ALLOWED_HOSTS` in `.env`:

       ALLOWED_HOSTS=localhost,127.0.0.1,[::1],<machine>.<tailnet>.ts.net

   Without the name, the request guard answers status 421 to every request that arrives at the address. Leave `APP_HOST_BIND` at `127.0.0.1`: Serve connects from the same computer.

3. Re-create the application container so that it reads the new value. All data stays:

       docker compose up -d

4. Check the proxy and the answer:

       tailscale serve status
       curl -s -o /dev/null -w '%{http_code}\n' https://<machine>.<tailnet>.ts.net/

   The status shows `https://<machine>.<tailnet>.ts.net (tailnet only)` with `/ proxy http://127.0.0.1:3000`. The request prints `200`.

5. Open `https://<machine>.<tailnet>.ts.net` on any device that is logged in to the tailnet.

# What persists

* Tailscale keeps the Serve configuration in its own state. It comes back when `tailscaled` restarts and after a reboot.
* Compose starts the containers again when Docker starts (`restart: unless-stopped`).
* Tailscale renews the certificate before it expires. Keep no copy of it in the project; `.gitignore` excludes `*.crt`, `*.key` and `*.pem`.

# Stop

Remove the proxy:

    tailscale serve reset

The dashboard stays open at `http://localhost:3000` on the computer. The name in `ALLOWED_HOSTS` can stay.

# Limits

* Serve gives transport security and tailnet membership, not a login. Every device on the tailnet can use the full API. See the [Security model](/concepts/security.md).
* Do not run `tailscale funnel` for this application.
