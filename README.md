# Codealong

Codealong is an open source tool to gain visibility into an engineering organization through source-level metrics. Examples of use cases include:

1. View and search pull requests and commits across a large number of repositories.
2. Run blame analysis to get breakdowns of code changes into new work, churn, and legacy refactors.
3. Create custom dashboards showing team and organization performance.
4. Search for patterns and anti-patterns (e.g. commits without tests, database migrations, etc.).
5. Detailed per-repository configuration related to tags, weights, etc.

Under the hood, Codealong is based on the [ELK](https://www.elastic.co/elk-stack) stack and uses [Kibana](https://www.elastic.co/products/kibana) for visualizations.

## Screenshots

TODO

## Getting Started

### 1. Install Rust and the Codealong CLI

First, install a stable version of rust. The recommended approach is through [rustup](https://rustup.rs/). Once installed, Codealong can be installed by running `cargo install codealong-cli` from your terminal. This will add the `codealong` binary to your path.

### 2. Create a Workspace

TODO

### 3. Setup Elasticsearch and Kibana

To store the results of the analysis, Codealong depends on an instance of Elasticsearch being accessible. The recommended approach is to use [docker](https://docs.docker.com/install/). To get started quickly, install docker locally and then create a file called `docker-compose.yml` in the workspace directory created in step 2:

```yaml
version: "3.1"

services:
  elasticsearch:
    image: docker.elastic.co/elasticsearch/elasticsearch-oss:6.6.0
    container_name: elasticsearch
    environment:
      - cluster.name=codealong-cluster
      - bootstrap.memory_lock=true
      - "ES_JAVA_OPTS=-Xms2048m -Xmx2048m"
    ulimits:
      memlock:
        soft: -1
        hard: -1
    volumes:
      - esdata:/usr/share/elasticsearch/data
    ports:
      - 9200:9200

  kibana:
    image: codealong-kibana:latest
    container_name: kibana
    ports:
      - 5601:5601

volumes:
  esdata:
    driver: local
```

### 4. Analyze

TODO

## Git and Github Credentials

TODO
