# ffmpeg-swarm

## Distrubuted ffmpeg with minimal configuration

![](./demo.webp "Demonstration video")

ffmpeg-swarm is still a work in progress. The windows binary is temporarily not working and some containers that require seeking are not yet supported.

### Features

- Zero network configuration
- Join a swarm in one command
- No master node: every node can submit a job

### Installation

```bash
cargo install --git https://github.com/DontBreakAlex/ffmpeg-swarm.git
```
(Requires cargo and ffmpeg)

```bash
docker run  -e TOKEN=<YOUR_TOKEN_HERE> git.alexandre-seo.com/dontbreakalex/ffmpeg-swarm:v0.1.4
```
(You will need to use the binary to generate a token)

### Usage

1. `ffmpeg-swarm configure` to generate a token
2. `ffmpeg-swarm server` to start your local node
3. `ffmpeg-swarm submit -- ffmpeg -i input.mp4 -vf "fps=30" output.mkv` to submit a job
4. `ffmpeg-swarm show-token` to display your swarm token

Run `ffmpeg-swarm join <TOKEN>` on another machine to join the swarm.

UPnP is required for your node to be reachable from outside your local network. If you don't know what UPnP is, your router probably has it enabled.