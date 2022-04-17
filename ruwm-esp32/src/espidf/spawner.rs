use esp_idf_svc::executor::asyncs::{local, sendable, EspLocalExecutor, EspSendableExecutor};

use ruwm::broadcast_binder::{Spawner, TaskPriority};

pub struct EspSpawner<'a> {
    high_prio: EspLocalExecutor<'a>,
    med_prio: EspSendableExecutor<'a>,
    low_prio: EspSendableExecutor<'a>,
}

impl<'a> EspSpawner<'a> {
    pub fn new(high_prio_tasks: usize, med_prio_tasks: usize, low_prio_tasks: usize) -> Self {
        Self {
            high_prio: local(high_prio_tasks),
            med_prio: sendable(med_prio_tasks),
            low_prio: sendable(low_prio_tasks),
        }
    }

    pub fn release(
        self,
    ) -> (
        EspLocalExecutor<'a>,
        EspSendableExecutor<'a>,
        EspSendableExecutor<'a>,
    ) {
        (self.high_prio, self.med_prio, self.low_prio)
    }
}

impl<'a> Spawner<'a> for EspSpawner<'a> {
    fn spawn(
        &mut self,
        priority: TaskPriority,
        fut: impl futures::Future<Output = ruwm::error::Result<()>> + Send + 'a,
    ) -> ruwm::error::Result<()> {
        match priority {
            TaskPriority::High => self.high_prio.spawn(fut).detach(),
            TaskPriority::Medium => self.med_prio.spawn(fut).detach(),
            TaskPriority::Low => self.low_prio.spawn(fut).detach(),
        }

        Ok(())
    }
}
