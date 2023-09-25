use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use futures_util::stream::FuturesOrdered;
use futures_util::{Future, Stream, StreamExt};
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::state::StateDiff;
use starknet_client::reader::{ReaderClientResult, StarknetReader, StateUpdate};
use tracing::log::trace;
use tracing::{debug, instrument};

use super::{ApiContractClass, CentralResult, CentralStateUpdate};
use crate::CentralError;

type TasksQueue<T> = FuturesOrdered<Pin<Box<dyn Future<Output = T> + Send>>>;
type NumberOfClasses = usize;

#[derive(Clone)]
pub struct StateUpdateStreamConfig {
    pub max_state_updates_to_download: usize,
    pub max_state_updates_to_store_in_memory: usize,
    pub max_classes_to_download: usize,
}

pub(crate) struct StateUpdateStream<TStarknetClient: StarknetReader + Send + 'static> {
    initial_block_number: BlockNumber,
    up_to_block_number: BlockNumber,
    starknet_client: Arc<TStarknetClient>,
    download_state_update_tasks: TasksQueue<(BlockNumber, ReaderClientResult<Option<StateUpdate>>)>,
    // Contains NumberOfClasses so we don't need to calculate it from the StateUpdate.
    downloaded_state_updates: VecDeque<(BlockNumber, NumberOfClasses, StateUpdate)>,
    classes_to_download: VecDeque<ClassHash>,
    download_class_tasks: TasksQueue<CentralResult<Option<ApiContractClass>>>,
    downloaded_classes: VecDeque<ApiContractClass>,
    config: StateUpdateStreamConfig,
}

impl<TStarknetClient: StarknetReader + Send + Sync + 'static> Stream
    for StateUpdateStream<TStarknetClient>
{
    type Item = CentralResult<CentralStateUpdate>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        loop {
            // In case an existing task is done or a new task is initiated, mark that we should poll
            // again.
            let mut should_poll_again = false;

            // Advances scheduling logic.
            if let Err(err) = self.do_scheduling(&mut should_poll_again, cx) {
                return Poll::Ready(Some(Err(err)));
            }

            // If available, returns the next block state update and corresponding classes.
            if let Some(maybe_central_state_update) = self.next_output() {
                return Poll::Ready(Some(maybe_central_state_update));
            }

            // In case no task is done and there are no newly initiated tasks, the stream is either
            // exhausted or no new item is available.
            if !should_poll_again {
                break;
            }
        }

        // The stream is exhausted.
        if self.download_class_tasks.is_empty() && self.download_state_update_tasks.is_empty() {
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}

impl<TStarknetClient: StarknetReader + Send + Sync + 'static> StateUpdateStream<TStarknetClient> {
    pub fn new(
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
        starknet_client: Arc<TStarknetClient>,
        config: StateUpdateStreamConfig,
    ) -> Self {
        StateUpdateStream {
            initial_block_number,
            up_to_block_number,
            starknet_client,
            download_state_update_tasks: futures::stream::FuturesOrdered::new(),
            downloaded_state_updates: VecDeque::with_capacity(
                config.max_state_updates_to_store_in_memory,
            ),
            classes_to_download: VecDeque::with_capacity(
                config.max_state_updates_to_store_in_memory * 5,
            ),
            download_class_tasks: futures::stream::FuturesOrdered::new(),
            downloaded_classes: VecDeque::with_capacity(
                config.max_state_updates_to_store_in_memory * 5,
            ),
            config,
        }
    }

    // Returns data needed for the next block CentralStateUpdate, or None if it is not yet ready.
    fn next_output(&mut self) -> Option<CentralResult<CentralStateUpdate>> {
        let (_, n_classes, _) = self.downloaded_state_updates.front()?;
        if self.downloaded_classes.len() < *n_classes {
            return None;
        }
        let (block_number, n_classes, state_update) =
            self.downloaded_state_updates.pop_front().expect("Should have a value");
        let class_hashes = state_update.state_diff.class_hashes();
        let classes = self.downloaded_classes.drain(..n_classes);
        let classes: IndexMap<ClassHash, ApiContractClass> =
            class_hashes.into_iter().zip(classes).collect();
        Some(client_to_central_state_update(block_number, Ok((state_update, classes))))
    }

    // Advances scheduling logic. Propagates errors to be returned from the stream.
    // - Schedules state update downloading tasks.
    // - For each downloaded state update: stores the result and the corresponding classes needed to
    //   be downloaded.
    // - Schedules class downloading tasks.
    // - For each downloaded class: stores the result.
    fn do_scheduling(
        self: &mut std::pin::Pin<&mut Self>,
        should_poll_again: &mut bool,
        cx: &mut std::task::Context<'_>,
    ) -> CentralResult<()> {
        self.schedule_class_downloads(should_poll_again);
        self.handle_downloaded_classes(cx, should_poll_again)?;
        self.schedule_state_update_downloads(should_poll_again);
        self.handle_downloaded_state_updates(cx, should_poll_again)?;
        Ok(())
    }

    // Adds more class downloading tasks.
    fn schedule_class_downloads(self: &mut std::pin::Pin<&mut Self>, should_poll_again: &mut bool) {
        while self.download_class_tasks.len() < self.config.max_classes_to_download {
            let Some(class_hash) = self.classes_to_download.pop_front() else {
                break;
            };
            let starknet_client = self.starknet_client.clone();
            self.download_class_tasks
                .push_back(Box::pin(download_class_if_necessary(class_hash, starknet_client)));
            *should_poll_again = true;
        }
    }

    // Checks for finished class downloading tasks and adds the result to `downloaded_classes`.
    fn handle_downloaded_classes(
        self: &mut std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        should_poll_again: &mut bool,
    ) -> CentralResult<()> {
        let Poll::Ready(Some(maybe_class)) = self.download_class_tasks.poll_next_unpin(cx) else {
            return Ok(());
        };

        *should_poll_again = true;
        match maybe_class {
            // Add to downloaded classes.
            Ok(Some(class)) => {
                self.downloaded_classes.push_back(class);
                Ok(())
            }
            // Class was not found.
            Ok(None) => Err(CentralError::ClassNotFound),
            // An error occurred while downloading the class.
            Err(err) => Err(err),
        }
    }

    // Adds more state update downloading tasks.
    fn schedule_state_update_downloads(
        self: &mut std::pin::Pin<&mut Self>,
        should_poll_again: &mut bool,
    ) {
        while self.initial_block_number < self.up_to_block_number
            && self.download_state_update_tasks.len() < self.config.max_state_updates_to_download
        {
            let current_block_number = self.initial_block_number;
            let starknet_client = self.starknet_client.clone();
            *should_poll_again = true;
            self.download_state_update_tasks.push_back(Box::pin(async move {
                (current_block_number, starknet_client.state_update(current_block_number).await)
            }));
            self.initial_block_number = self.initial_block_number.next();
        }
    }

    // Checks for finished state update downloading tasks.
    // Checks for finished class downloading tasks and adds the result to `downloaded_classes`.
    fn handle_downloaded_state_updates(
        self: &mut std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        should_poll_again: &mut bool,
    ) -> CentralResult<()> {
        if self.downloaded_state_updates.len() >= self.config.max_state_updates_to_store_in_memory {
            return Ok(());
        }

        let Poll::Ready(Some((block_number, maybe_state_update))) =
            self.download_state_update_tasks.poll_next_unpin(cx)
        else {
            return Ok(());
        };

        *should_poll_again = true;
        match maybe_state_update {
            // Add to downloaded state updates. Adds the results to `downloaded_state_updates` and
            // the corresponding classes needed to   be downloaded to `classes_to_download`.
            Ok(Some(state_update)) => {
                let hashes = state_update.state_diff.class_hashes();
                let n_classes = hashes.len();
                self.classes_to_download.append(&mut VecDeque::from(hashes));
                self.downloaded_state_updates.push_back((block_number, n_classes, state_update));
                Ok(())
            }
            // Class was not found.
            Ok(None) => Err(CentralError::ClassNotFound),
            // An error occurred while downloading the class.
            Err(err) => Err(CentralError::ClientError(err.into())),
        }
    }
}

fn client_to_central_state_update(
    current_block_number: BlockNumber,
    maybe_client_state_update: CentralResult<(StateUpdate, IndexMap<ClassHash, ApiContractClass>)>,
) -> CentralResult<CentralStateUpdate> {
    match maybe_client_state_update {
        Ok((state_update, mut declared_classes)) => {
            // Destruct the state diff to avoid partial move.
            let starknet_client::reader::StateDiff {
                storage_diffs,
                deployed_contracts,
                declared_classes: declared_class_hashes,
                old_declared_contracts: old_declared_contract_hashes,
                nonces,
                replaced_classes,
            } = state_update.state_diff;

            // Separate the declared classes to new classes, old classes and classes of deployed
            // contracts (both new and old).
            let n_declared_classes = declared_class_hashes.len();
            let mut deprecated_classes = declared_classes.split_off(n_declared_classes);
            let n_deprecated_declared_classes = old_declared_contract_hashes.len();
            let deployed_contract_class_definitions =
                deprecated_classes.split_off(n_deprecated_declared_classes);

            let state_diff = StateDiff {
                deployed_contracts: IndexMap::from_iter(
                    deployed_contracts.iter().map(|dc| (dc.address, dc.class_hash)),
                ),
                storage_diffs: IndexMap::from_iter(storage_diffs.into_iter().map(
                    |(address, entries)| {
                        (address, entries.into_iter().map(|se| (se.key, se.value)).collect())
                    },
                )),
                declared_classes: declared_classes
                    .into_iter()
                    .map(|(class_hash, class)| {
                        (class_hash, class.into_cairo1().expect("Expected Cairo1 class."))
                    })
                    .zip(
                        declared_class_hashes
                            .into_iter()
                            .map(|hash_entry| hash_entry.compiled_class_hash),
                    )
                    .map(|((class_hash, class), compiled_class_hash)| {
                        (class_hash, (compiled_class_hash, class))
                    })
                    .collect(),
                deprecated_declared_classes: deprecated_classes
                    .into_iter()
                    .map(|(class_hash, generic_class)| {
                        (class_hash, generic_class.into_cairo0().expect("Expected Cairo0 class."))
                    })
                    .collect(),
                nonces,
                replaced_classes: replaced_classes
                    .into_iter()
                    .map(|replaced_class| (replaced_class.address, replaced_class.class_hash))
                    .collect(),
            };
            // Filter out deployed contracts of new classes because since 0.11 new classes can not
            // be implicitly declared by deployment.
            let deployed_contract_class_definitions = deployed_contract_class_definitions
                .into_iter()
                .filter_map(|(class_hash, contract_class)| match contract_class {
                    ApiContractClass::DeprecatedContractClass(deprecated_contract_class) => {
                        Some((class_hash, deprecated_contract_class))
                    }
                    ApiContractClass::ContractClass(_) => None,
                })
                .collect();
            let block_hash = state_update.block_hash;
            debug!(
                "Received new state update of block {current_block_number} with hash {block_hash}."
            );
            trace!(
                "State diff: {state_diff:?}, deployed_contract_class_definitions: \
                 {deployed_contract_class_definitions:?}."
            );
            Ok((current_block_number, block_hash, state_diff, deployed_contract_class_definitions))
        }
        Err(err) => {
            debug!("Received error for state diff {}: {:?}.", current_block_number, err);
            Err(err)
        }
    }
}

// Given a class hash, returns the corresponding class definition.
// First tries to retrieve the class from the storage.
// If not found in the storage, the class is downloaded.
#[instrument(skip(starknet_client), level = "debug", err)]
async fn download_class_if_necessary<TStarknetClient: StarknetReader>(
    class_hash: ClassHash,
    starknet_client: Arc<TStarknetClient>,
) -> CentralResult<Option<ApiContractClass>> {
    trace!("Downloading class {:?}.", class_hash);
    let client_class = starknet_client.class_by_hash(class_hash).await.map_err(Arc::new)?;
    match client_class {
        None => Ok(None),
        Some(class) => Ok(Some(class.into())),
    }
}
