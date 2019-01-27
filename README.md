# Codealong

Codealong is a tool to gain visibility into all repos

## Getting Started With a Github Organization

First, create a working directory and generate a configuration YAML file via the API:

```
mkdir ~/codealong
cd ~/codealong
codealong init --github-repo YOUR_GITHUB_ORGANIZATION
```

This will create a file in the directory, `.codealong/config.yml`. In order to maximize the utility of codealong, this file should be curated to accurately reflect the organization. The file will already be populated with all github users within the `authors` key:
