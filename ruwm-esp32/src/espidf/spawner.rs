use esp_idf_svc::executor::asyncs::{local, sendable, EspLocalExecutor, EspSendableExecutor};

use ruwm::{
    broadcast_binder::{Spawner, TaskPriority},
    error,
};
use smol::Task;

pub struct EspSpawner<'a> {
    high_prio: EspLocalExecutor<'a>,
    med_prio: EspSendableExecutor<'a>,
    low_prio: EspSendableExecutor<'a>,
    high_prio_tasks: Vec<Task<error::Result<()>>>,
    med_prio_tasks: Vec<Task<error::Result<()>>>,
    low_prio_tasks: Vec<Task<error::Result<()>>>,
}

impl<'a> EspSpawner<'a> {
    pub fn new(high_prio_tasks: usize, med_prio_tasks: usize, low_prio_tasks: usize) -> Self {
        Self {
            high_prio: local(high_prio_tasks),
            med_prio: sendable(med_prio_tasks),
            low_prio: sendable(low_prio_tasks),
            high_prio_tasks: Vec::with_capacity(high_prio_tasks),
            med_prio_tasks: Vec::with_capacity(med_prio_tasks),
            low_prio_tasks: Vec::with_capacity(low_prio_tasks),
        }
    }

    pub fn release(
        self,
    ) -> (
        (EspLocalExecutor<'a>, Vec<Task<error::Result<()>>>),
        (EspSendableExecutor<'a>, Vec<Task<error::Result<()>>>),
        (EspSendableExecutor<'a>, Vec<Task<error::Result<()>>>),
    ) {
        (
            (self.high_prio, self.high_prio_tasks),
            (self.med_prio, self.med_prio_tasks),
            (self.low_prio, self.low_prio_tasks),
        )
    }
}

impl<'a> Spawner<'a> for EspSpawner<'a> {
    fn spawn(
        &mut self,
        priority: TaskPriority,
        fut: impl futures::Future<Output = ruwm::error::Result<()>> + Send + 'a,
    ) -> ruwm::error::Result<()> {
        self.high_prio_tasks.push(self.high_prio.spawn(fut));
        // match priority {
        //     TaskPriority::High => self.high_prio_tasks.push(self.high_prio.spawn(fut)),
        //     TaskPriority::Medium => self.med_prio_tasks.push(self.med_prio.spawn(fut)),
        //     TaskPriority::Low => self.low_prio_tasks.push(self.low_prio.spawn(fut)),
        // };

        Ok(())
    }
}
