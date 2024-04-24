# Dump Declared Classes Tool

This tool allows you to dump the entire `declared_classes` table from Papyrus storage into a file.

## Instructions

1. **Run a Docker**
   Please refer to the main [README](../../../../README.adoc#running-papyrus-with-docker) for instructions.

   Note: use a released Docker image

3. **View Running Docker Containers**

   ```bash
   docker ps
   ```
   You can also view the logs produced by the full node with:

   ```bash
   docker logs <docker_id>
   ```

4. **Sync the Full Node**

   The full node sync could take a few hours/days. Once it's partially or fully synced, you can run the tool to dump the declared classes into a file.

5. **Access the Docker Container**

   ```bash
   docker exec -ti <docker_id> sh
   ```

6. **Run the Tool**

   ```bash
   target/release/dump_declared_classes --start_block <block_number> --end_block <block_number> --chain_id <SN_MAIN/SN_SEPOLIA> [--file_path file_path]
   ```

   The default value for file_path is `dump_declared_classes.json`.


