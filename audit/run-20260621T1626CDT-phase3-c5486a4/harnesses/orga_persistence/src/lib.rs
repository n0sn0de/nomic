#![feature(specialization)]
#![feature(trivial_bounds)]

use orga::call::{build_call, Call};
use orga::encoding::{Decode, Encode};
use orga::plugins::{ABCICall, ABCIPlugin};
use orga::prelude::*;
use orga::state::State;
use orga::store::{BackingStore, BufStore, MapStore, Read, Shared, Store, Write};
use orga::{orga, Error, Result};
use std::marker::PhantomData;
use std::panic::catch_unwind;
use std::sync::Mutex;
use tendermint_proto::v0_34::abci::{RequestDeliverTx, ResponseDeliverTx};

type WrappedMap = Shared<BufStore<Shared<BufStore<Shared<MapStore>>>>>;

trait AuditApp: State + Call + Query + Default {}
impl<T: State + Call + Query + Default> AuditApp for T {}

#[orga]
pub struct ToyApp {
    pub counter: u32,
}

#[orga]
impl ToyApp {
    #[call]
    pub fn mutate_then_error(&mut self) -> Result<()> {
        self.counter += 1;
        Err(Error::App("boom after mutation".to_string()))
    }
}

/// Audit-local copy of the relevant private Orga `InternalApp` source order.
///
/// The real type is private inside `orga::abci::node`, so this harness mirrors
/// the public-code path with public `ABCIPlugin`, `State`, and store APIs.
struct AuditInternalApp<A> {
    _app: PhantomData<A>,
}

impl<A: AuditApp> AuditInternalApp<ABCIPlugin<A>> {
    fn new() -> Self {
        Self { _app: PhantomData }
    }

    fn run<T, F: FnOnce(&Mutex<ABCIPlugin<A>>) -> T>(&self, store: WrappedMap, op: F) -> Result<T> {
        let mut store = Store::new(BackingStore::Other(Shared::new(Box::new(store))));
        let state_bytes = match store.get(&[])? {
            Some(inner) => inner,
            None => {
                let mut default: ABCIPlugin<A> = Default::default();
                default.attach(store.clone())?;
                let mut encoded_bytes = vec![];
                default.flush(&mut encoded_bytes)?;

                store.put(vec![], encoded_bytes.clone())?;
                encoded_bytes
            }
        };
        let state: Mutex<ABCIPlugin<A>> = Mutex::new(ABCIPlugin::<A>::load(
            store.clone(),
            &mut state_bytes.as_slice(),
        )?);
        let res = op(&state);
        if let Ok(state) = state.into_inner() {
            let mut bytes = vec![];
            state.flush(&mut bytes)?;
            store.put(vec![], bytes)?;
        }
        Ok(res)
    }
}

impl<A: AuditApp> AuditInternalApp<ABCIPlugin<A>> {
    fn deliver_tx(&self, store: WrappedMap, req: RequestDeliverTx) -> Result<ResponseDeliverTx> {
        let run_res: Result<(Result<Result<()>>, Vec<String>, Vec<String>)> =
            self.run(store, move |state| -> Result<_> {
                let res = catch_unwind(|| {
                    let inner_call = Decode::decode(req.tx.to_vec().as_slice())?;
                    state.lock().unwrap().call(ABCICall::DeliverTx(inner_call))
                })
                .map_err(|_| Error::Call("Panicked".to_string()));

                Ok((res, Vec::<String>::new(), Vec::<String>::new()))
            })?;

        let mut deliver_tx_res = ResponseDeliverTx::default();
        match run_res {
            Ok((res, _events, logs)) => match res {
                Ok(Ok(())) => {
                    deliver_tx_res.code = 0;
                    deliver_tx_res.log = logs.join("\n");
                }
                Err(err) | Ok(Err(err)) => {
                    deliver_tx_res.code = 1;
                    if logs.is_empty() {
                        deliver_tx_res.log = err.to_string();
                    } else {
                        deliver_tx_res.log = logs.join("\n");
                    }
                }
            },
            Err(err) => {
                deliver_tx_res.code = 1;
                deliver_tx_res.log = err.to_string();
            }
        }

        Ok(deliver_tx_res)
    }
}

fn load_counter(store: Shared<MapStore>) -> Result<u32> {
    let store_for_read = Store::new(BackingStore::from(store));
    let bytes = store_for_read
        .get(&[])?
        .ok_or_else(|| Error::App("missing root state".to_string()))?;
    let loaded = ABCIPlugin::<ToyApp>::load(store_for_read, &mut bytes.as_slice())?;
    Ok(loaded.inner.counter)
}

#[test]
fn failed_deliver_tx_persists_mutation_in_audit_internal_app_source_order() -> Result<()> {
    let map = Shared::new(MapStore::new());
    let mut consensus = Shared::new(BufStore::wrap(map.clone()));
    let mut flush = Shared::new(BufStore::wrap(consensus.clone()));
    let app = AuditInternalApp::<ABCIPlugin<ToyApp>>::new();

    let toy = Box::new(ToyApp::default());
    let call = build_call!(toy.mutate_then_error());
    let req = RequestDeliverTx {
        tx: call.encode()?.into(),
    };

    let res = app.deliver_tx(flush.clone(), req)?;
    assert_eq!(res.code, 1);
    assert!(res.log.contains("boom after mutation"));

    flush.borrow_mut().flush()?;
    consensus.borrow_mut().flush()?;

    let counter = load_counter(map)?;
    assert_eq!(counter, 1);

    Ok(())
}
