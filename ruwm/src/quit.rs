use channel_bridge::notification::Notification;

const INIT: Notification = Notification::new();

pub static QUIT: [Notification; 3] = [INIT; 3];
