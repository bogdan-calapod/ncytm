## Resource Footprint

> **Note**: This comparison is from the original ncspot project comparing against the Spotify client. Resource footprint for ncytm with YouTube Music backend is TBD.

### Original ncspot vs Spotify Client
Measured using `ps_mem` on Linux during playback:

| Client  | Private Memory | Shared Memory | Total      |
|---------|----------------|---------------|------------|
| ncspot  | 22.1 MiB       | 24.1 MiB      | 46.2 MiB   |
| Spotify | 407.3 MiB      | 592.7 MiB     | 1000.0 MiB |
