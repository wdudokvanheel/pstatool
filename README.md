# Pstatool

Project Stats Tool (_pstatool_) automates the retrieval, analysis, and visualization of GitHub repositories. It clones 
repositories, runs CLOC to gather code statistics, and generates SVG visualizations. Designed 
for automation, it includes a Docker container that periodically updates projects, generates 
visualizations, and serves them via a built-in web server.

## Features
- Shallow clones Git repositories for efficient analysis
- Generates SVG visualizations of code metrics
- Stores project statistics in a PostgreSQL database for further processing
- Docker container includes a web server for hosting of SVG files

## How to run
The binary can be run to retrieve all projects from the database and generate SVG files for each.

    pstatool --db-url <DB_URL> --svg-folder <SVG_FOLDER> --temp-folder <TEMP_FOLDER>

### Docker compose
```
version: '3.0'
services:
    postgres:
        image: postgres:17.3
        container_name: pstatool-db
        restart: unless-stopped
        environment:
            POSTGRES_USER: "pstatool"
            POSTGRES_PASSWORD: "pstatool"
            POSTGRES_DB: "pstatool"
        volumes:
            - pstatool:/var/lib/postgresql/data
    pstatool:
        image: pstatool:latest
        container_name: pstatool
        restart: unless-stopped
        depends_on:
            - postgres
        ports:
        - "80:80"
        environment:
            DB_URL: "postgresql://pstatool:pstatool@pstatool-db:5432/pstatool"
            
volumes:
    pstatool: 
```

### Webserver

The generated SVG files are hosted by the Docker container (with nginx) at the path `githubuser/project-name.svg`
For example, this repository have its stats SVG at `http://localhost/wdudokvanheel/pstatool.svg` 


## Example

These are the generated stats for this repository

![stats](https://pstatool.wdudokvanheel.nl/wdudokvanheel/pstatool.svg)

To Do
- [ ] Sum SVG for all projects
- [ ] Allow binary generate svg of target folder (no database option)
- [ ] Allow binary generate svg of target git repo (no database option)
