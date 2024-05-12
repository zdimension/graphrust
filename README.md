# graphrust

Graph viewer

![image](https://github.com/zdimension/graphrust/assets/4533568/41481c0b-ea08-4a0c-94ed-d3aed83ec914)

## Importer

The import tool fetches the data from the Neo4j backend and performs multiple analysis passes:

- Community
  detection (Louvain) on
  GPU: [fork](https://github.com/zdimension/gpu-louvain), [original](https://github.com/olearczuk/gpu-louvain)
- Graph layout (ForceAtlas2) on
  GPU: [fork](https://github.com/zdimension/GPUGraphLayout), [original](https://github.com/govertb/GPUGraphLayout)

## Viewer

The viewer uses [egui](https://github.com/emilk/egui) for the
UI, [zearch](https://github.com/irevoire/zearch) for the search (which was specially crafted for this use case!), and a
handwritten OpenGL renderer
using [glow](https://github.com/grovesNL/glow/).