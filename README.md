*currently moving things from `server/` to `textual/`, slowly*

**eventaul repository layout**
- **textual:** the actual image generation and code lives here
- **server:** hyper powered, conventional web server. more full featured than the edge version
- **fastly:** a compute@edge application for rendering images on the edge (demo)
- **fastly-backend:** font provider for the fastly compute application
