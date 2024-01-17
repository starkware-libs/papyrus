use libp2p::Swarm;

use crate::PapyrusBehaviour;

pub struct NetworkManager<Behaviour>
where
    Behaviour: PapyrusBehaviour,
{
    #[allow(dead_code)]
    swarm: Swarm<Behaviour>,
}

impl<Behaviour> NetworkManager<Behaviour>
where
    Behaviour: PapyrusBehaviour,
{
    pub fn new(swarm: Swarm<Behaviour>) -> Self {
        Self { swarm }
    }
    pub async fn run(self) {
        unimplemented!()
    }
}
