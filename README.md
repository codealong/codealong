# Codealong

Codealong is an open source tool to gain visibility into an engineering organization through source-level metrics. Examples of use cases include:

1. View and search pull requests and commits across a large number of repositories.
2. Run blame analysis to get breakdowns of code changes into new work, churn, and legacy refactors.
3. Create custom dashboards showing team and organization performance.
4. Search for patterns and anti-patterns (e.g. commits without tests, database migrations, etc.).
5. Detailed per-repository configuration related to tags, weights, etc.

Under the hood, Codealong is based on the [ELK](https://www.elastic.co/elk-stack) stack and uses [Kibana](https://www.elastic.co/products/kibana) for visualizations.

_WARNING_: This software is considered alpha and will most likely change significantly over time.

![Dashboard](/screenshots/dashboard.png?raw=true)

## Getting Started

### 1. Install Rust and the Codealong CLI

First, install a stable version of rust. The recommended approach is through [rustup](https://rustup.rs/). Once installed, Codealong can be installed by running `cargo install codealong-cli` from your terminal. This will add the `codealong` binary to your path.

### 2. Create a Workspace

A workspace is a directory that is responsible for two things:

1. Storing working checkouts of repos to be analyzed
2. Maintaining a workspace-level configuration file, `config.yml`

To create a workspace, create a directory and then run the `codealong init` subcommand:

```bash
mkdir ~/codealong
cd ~/codealong
codealong init . --github-org YOUR_GITHUB_ORGANIZATION
```

Note that in the above commands, `YOUR_GITHUB_ORGANIZATION` should be replaced with the Github organization containing the users and repos to be analyzed. Mulitple organization's can be specified by specifying multiple `--github-org` arguments. As part of the initialization, information about the organization and the users will be crawled via the Github API. The `config.yml` file can also be manually modified to include specific repos.

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
    image: codealong/codealong-kibana:latest
    container_name: kibana
    ports:
      - 5601:5601

volumes:
  esdata:
    driver: local
```

Within the workspace directory, run `docker-compose up` to start Elasticsearch and Kibana. The above image, [codealong/codealong-kibana](https://cloud.docker.com/u/codealong/repository/docker/codealong/codealong-kibana), is a custom kibana image containing some pre-made visualizations and dashboards.

### 4. Run the analyze subcommand

Run the following command from within the workspace directory:

```
codealong analyze -w . --skip-forks -p --since 3months
```

This will clone/fetch all relevant repos and then walk the revision tree and analyze each commit and pull request and store them in Elasticsearch. Run `codealong analyze -h` for more information on each of the flags.

The `analyze` subcommand is idempotent and can be re-run to pick up new commits and configuration changes.

### 5. Visualize via Kibana

After or during the step 4, go to [http://localhost:5601](http://localhost:5601) to view the kibana dashboard. If you used the `codealong/codealong-kibana` docker image, there should be some prebuilt visualizations and dashboards.

## Git and Github Credentials

In order to checkout private repos, ensure that your private SSH key is added to your ssh-agent.

To examine the pull requests of private repos, create a [Github personal access token](https://help.github.com/articles/creating-a-personal-access-token-for-the-command-line/) and store it an environment variable called `GITHUB_TOKEN`.

## Configuration

More information soon, but for now the [source documentation](https://docs.rs/codealong/latest/codealong/struct.Config.html) is the best bet.
