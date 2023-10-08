# Dump Declared Classes Tool

This tool allows you to dump the entire `declared_classes` table from Papyrus storage into a file.

## Instructions

1. **Build Papyrus Docker Image**

   ```bash
   docker build . -t <image_name>
   ```
   This stage may take a few minutes.

2. **Run the Docker Image**

   ```bash
   docker run --rm --name papyrus -p 8080-8081:8080-8081 -v <path_in_your_local_machine>/app/data:/app/data <image:latest --base_layer.node_url <ethereum mainnet node url>
   ```

   > **Note**: Ensure the directory `<path_in_your_local_machine>/app/data` has write permissions so the Docker container can write to the database.

3. **View Running Docker Containers**

   ```bash
   docker ps
   ```
   You can also view the logs produced by the full node with:

   ```bash
   docker logs <docker_id>
   ```

4. **Sync the Full Node**

   The full node sync could take a few hours. Once it's partially or fully synced, you can run the tool to dump the declared classes into a file.

5. **Access the Docker Container**

   ```bash
   docker exec -ti <docker_id> sh
   ```

6. **Run the Tool**

   ```bash
   target/release/dump_declared_classes [file_path]
   ```

   The `file_path` is optional. The default path is `dump_declared_classes.json`.


