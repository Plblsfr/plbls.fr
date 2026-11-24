use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotifySettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<BTreeMap<String, NotifySettingsLevelsValeur>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<NotifySettingsTypes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_group: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub general: Option<NotifySettingsGeneral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutes: Option<BTreeMap<String, i64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotifySettingsLevelsValeur {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "mentions")]
    Mentions,
    #[serde(rename = "none")]
    None,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotifySettingsTypes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotifySettingsGeneral {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "mentions")]
    Mentions,
    #[serde(rename = "none")]
    None,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Error {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Person {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pseudo: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Server {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_version: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ChannelKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowmode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_limit: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelKind {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "voice")]
    Voice,
    #[serde(rename = "videos")]
    Videos,
    #[serde(rename = "music")]
    Music,
    #[serde(rename = "cinema")]
    Cinema,
    #[serde(other)]
    Inconnu,
}

pub type Permissions = Vec<String>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Role {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoleInput {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Override {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Permissions>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerInvite {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Invitation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<AttachmentKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waveform: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_auto: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttachmentKind {
    #[serde(rename = "file")]
    File,
    #[serde(rename = "voice")]
    Voice,
    #[serde(rename = "video")]
    Video,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_auto: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub me: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<Person>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Person>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<Attachment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactions: Option<Vec<Reaction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<MessageReplyTo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<MessagePoll>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageReplyTo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Person>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessagePoll {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<Person>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<MessagePollOptionsElement>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessagePollOptionsElement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub votes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub me: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DM {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<Person>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mention {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Person>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LinkPreview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sillon: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Name50 {
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NewMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessagePage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetMeReponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rights: Option<Vec<GetMeReponseRightsElement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<BTreeMap<String, GetMeReponseCapabilitiesValeur>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cinema: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_video_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<GetMeReponseTranscription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pseudo: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GetMeReponseRightsElement {
    #[serde(rename = "access")]
    Access,
    #[serde(rename = "admin")]
    Admin,
    #[serde(rename = "acces")]
    Acces,
    #[serde(rename = "users")]
    Users,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetMeReponseCapabilitiesValeur {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GetMeReponseTranscription {
    #[serde(rename = "")]
    Vide,
    #[serde(rename = "alpha")]
    Alpha,
    #[serde(rename = "on")]
    On,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostActivateRequete {
    pub pseudo: String,
}

pub type PostActivateReponse = Person;

pub type GetNotificationsReponse = Vec<Mention>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostNotificationsReadRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetUnreadReponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dms: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<BTreeMap<String, i64>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetChannelsCidReadsReponseElement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pseudo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
}

pub type GetChannelsCidReadsReponse = Vec<GetChannelsCidReadsReponseElement>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetDmsIdReadsReponseElement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pseudo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
}

pub type GetDmsIdReadsReponse = Vec<GetDmsIdReadsReponseElement>;

pub type GetAvatarsReponse = BTreeMap<String, i64>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetPushKeyReponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutPushFcmRequete {
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeletePushFcmRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutPushDesktopRequete {
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeletePushDesktopRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutPushSubscriptionRequete {
    pub endpoint: String,
    pub keys: PutPushSubscriptionRequeteKeys,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutPushSubscriptionRequeteKeys {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p256dh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeletePushSubscriptionRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PatchServersIdEmojisEidRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostGifsImportRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostAdminBadgesRequete {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostAdminBadgesIdUsersRequete {
    pub pseudos: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostAdminBotMessagesRequete {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetInternalProfileVisibleReponse {
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostReportsRequete {
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<PostReportsRequeteCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<PostReportsRequetePlatform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tech: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostReportsRequeteCategory {
    #[serde(rename = "messages")]
    Messages,
    #[serde(rename = "vocal")]
    Vocal,
    #[serde(rename = "ecran")]
    Ecran,
    #[serde(rename = "notifications")]
    Notifications,
    #[serde(rename = "autre")]
    Autre,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostReportsRequetePlatform {
    #[serde(rename = "web")]
    Web,
    #[serde(rename = "pwa")]
    Pwa,
    #[serde(rename = "desktop")]
    Desktop,
    #[serde(rename = "android")]
    Android,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchAdminReportsIdRequete {
    pub status: PatchAdminReportsIdRequeteStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatchAdminReportsIdRequeteStatus {
    #[serde(rename = "new")]
    New,
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "resolved")]
    Resolved,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "duplicate")]
    Duplicate,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutMeStatusRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PutMeStatusRequeteStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PutMeStatusRequeteStatus {
    #[serde(rename = "online")]
    Online,
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "dnd")]
    Dnd,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutMeBannerRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutMeActivityRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

pub type GetMeNotifySettingsReponse = NotifySettings;

pub type PutMeNotifySettingsRequete = NotifySettings;

pub type GetServersReponse = Vec<Server>;

pub type PostServersRequete = Name50;

pub type PostServersReponse = Server;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetServersIdReponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<Server>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<GetServersIdReponseChannelsElement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<GetServersIdReponseMembersElement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<Role>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetServersIdReponseChannelsElement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<GetServersIdReponseChannelsElementKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowmode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GetServersIdReponseChannelsElementKind {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "voice")]
    Voice,
    #[serde(rename = "videos")]
    Videos,
    #[serde(rename = "music")]
    Music,
    #[serde(rename = "cinema")]
    Cinema,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetServersIdReponseMembersElement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pseudo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner_color: Option<String>,
}

pub type PatchServersIdRequete = Name50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostServersIdChannelsRequete {
    pub name: String,
    pub kind: PostServersIdChannelsRequeteKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowmode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostServersIdChannelsRequeteKind {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "voice")]
    Voice,
    #[serde(rename = "videos")]
    Videos,
    #[serde(rename = "music")]
    Music,
    #[serde(rename = "cinema")]
    Cinema,
    #[serde(other)]
    Inconnu,
}

pub type PostServersIdChannelsReponse = Channel;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PutServersIdChannelsOrderRequete {
    pub kind: PutServersIdChannelsOrderRequeteKind,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PutServersIdChannelsOrderRequeteKind {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "voice")]
    Voice,
    #[serde(rename = "videos")]
    Videos,
    #[serde(rename = "music")]
    Music,
    #[serde(rename = "cinema")]
    Cinema,
    #[serde(other)]
    Inconnu,
}

pub type PatchServersIdChannelsCidRequete = Name50;

pub type GetServersIdInvitesReponse = Vec<ServerInvite>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostServersIdInvitesRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
}

pub type PostServersIdInvitesReponse = ServerInvite;

pub type PostServersIdRolesRequete = RoleInput;

pub type PostServersIdRolesReponse = Role;

pub type PatchServersIdRolesRidRequete = RoleInput;

pub type PatchServersIdRolesRidReponse = Role;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutServersIdRolesOrderRequete {
    pub ids: Vec<String>,
}

pub type GetServersIdChannelsCidOverridesReponse = Vec<Override>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutServersIdChannelsCidOverridesRidRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Permissions>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetServersIdAuditLogReponseElement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<GetServersIdAuditLogReponseElementAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GetServersIdAuditLogReponseElementAction {
    #[serde(rename = "role.create")]
    RoleCreate,
    #[serde(rename = "role.update")]
    RoleUpdate,
    #[serde(rename = "role.delete")]
    RoleDelete,
    #[serde(rename = "role.reorder")]
    RoleReorder,
    #[serde(rename = "role.assign")]
    RoleAssign,
    #[serde(rename = "role.unassign")]
    RoleUnassign,
    #[serde(rename = "channel.override")]
    ChannelOverride,
    #[serde(rename = "channel.moderation")]
    ChannelModeration,
    #[serde(rename = "member.kick")]
    MemberKick,
    #[serde(rename = "member.ban")]
    MemberBan,
    #[serde(rename = "member.unban")]
    MemberUnban,
    #[serde(rename = "member.timeout")]
    MemberTimeout,
    #[serde(rename = "member.untimeout")]
    MemberUntimeout,
    #[serde(rename = "member.warn")]
    MemberWarn,
    #[serde(rename = "member.nickname")]
    MemberNickname,
    #[serde(rename = "voice.move")]
    VoiceMove,
    #[serde(rename = "voice.disconnect")]
    VoiceDisconnect,
    #[serde(rename = "message.delete")]
    MessageDelete,
    #[serde(rename = "message.bulk_delete")]
    MessageBulkDelete,
    #[serde(rename = "server.transfer")]
    ServerTransfer,
    #[serde(other)]
    Inconnu,
}

pub type GetServersIdAuditLogReponse = Vec<GetServersIdAuditLogReponseElement>;

pub type PostServerInvitesCodeJoinReponse = Server;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostServersIdMembersUidKickRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostServersIdMembersUidBanRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_messages: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetServersIdBansReponseElement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pseudo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

pub type GetServersIdBansReponse = Vec<GetServersIdBansReponseElement>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutServersIdMembersUidTimeoutRequete {
    pub duration: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostServersIdMembersUidWarnRequete {
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutServersIdMembersUidNicknameRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutServersIdChannelsCidModerationRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowmode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostChannelsCidMessagesBulkDeleteRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostServersIdTransferRequete {
    pub user_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidParticipantsUidMoveRequete {
    pub channel_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GetServersIdVoiceReponseValeurElement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pseudo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

pub type GetServersIdVoiceReponse = BTreeMap<String, Vec<GetServersIdVoiceReponseValeurElement>>;

pub type GetServersIdVoiceStatsReponse = BTreeMap<String, i64>;

pub type GetServersIdAvatarsReponse = BTreeMap<String, i64>;

pub type GetChannelsCidMessagesReponse = MessagePage;

pub type PostChannelsCidMessagesRequete = NewMessage;

pub type PostChannelsCidMessagesReponse = Message;

pub type GetDmsReponse = Vec<DM>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostDmsRequete {
    pub pseudo: String,
}

pub type PostDmsReponse = DM;

pub type GetDmsIdMessagesReponse = MessagePage;

pub type PostDmsIdMessagesRequete = NewMessage;

pub type PostDmsIdMessagesReponse = Message;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PatchMessagesIdRequete {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
}

pub type PatchMessagesIdReponse = Message;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutMessagesIdVoteRequete {
    pub option_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeleteMessagesIdVoteRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_id: Option<String>,
}

pub type GetMessagesIdPollVotesReponse = BTreeMap<String, Vec<Person>>;

pub type PostFilesReponse = Attachment;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostFilesIdLinkReponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

pub type PostFilesIdTranscriptReponse = Transcript;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutFilesIdTranscriptRequete {
    pub text: String,
}

pub type PutFilesIdTranscriptReponse = Transcript;

pub type GetLinkPreviewReponse = LinkPreview;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidTokenReponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_participants: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_max_fps: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_max_height: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidWatchRequete {
    pub video: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidWatchActionRequete {
    pub action: PostVoiceCidWatchActionRequeteAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostVoiceCidWatchActionRequeteAction {
    #[serde(rename = "play")]
    Play,
    #[serde(rename = "pause")]
    Pause,
    #[serde(rename = "seek")]
    Seek,
    #[serde(rename = "rate")]
    Rate,
    #[serde(rename = "previous")]
    Previous,
    #[serde(rename = "shuffle")]
    Shuffle,
    #[serde(rename = "ended")]
    Ended,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidWatchReadyRequete {
    pub ready: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidWatchOptionsRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<PutVoiceCidWatchOptionsRequeteRepeat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PutVoiceCidWatchOptionsRequeteRepeat {
    #[serde(rename = "")]
    Vide,
    #[serde(rename = "one")]
    One,
    #[serde(rename = "all")]
    All,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidWatchQueueRequete {
    pub video: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidWatchQueueRequete {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidDrawSettingsRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rounds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_secs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_choices: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_players: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<PutVoiceCidDrawSettingsRequeteLang>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_words: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PutVoiceCidDrawSettingsRequeteLang {
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "en")]
    En,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidDrawChooseRequete {
    pub index: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidDrawGuessRequete {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidDrawDrawRequete {
    pub op: PostVoiceCidDrawDrawRequeteOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<PostVoiceCidDrawDrawRequeteTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pts: Option<Vec<Vec<f64>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostVoiceCidDrawDrawRequeteOp {
    #[serde(rename = "stroke")]
    Stroke,
    #[serde(rename = "undo")]
    Undo,
    #[serde(rename = "clear")]
    Clear,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostVoiceCidDrawDrawRequeteTool {
    #[serde(rename = "pen")]
    Pen,
    #[serde(rename = "eraser")]
    Eraser,
    #[serde(rename = "fill")]
    Fill,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidDrawVoteRequete {
    pub kind: PostVoiceCidDrawVoteRequeteKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostVoiceCidDrawVoteRequeteKind {
    #[serde(rename = "report")]
    Report,
    #[serde(rename = "skip")]
    Skip,
    #[serde(rename = "kick")]
    Kick,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidMusicRequete {
    pub video: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidMusicActionRequete {
    pub action: PostVoiceCidMusicActionRequeteAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostVoiceCidMusicActionRequeteAction {
    #[serde(rename = "play")]
    Play,
    #[serde(rename = "pause")]
    Pause,
    #[serde(rename = "seek")]
    Seek,
    #[serde(rename = "rate")]
    Rate,
    #[serde(rename = "previous")]
    Previous,
    #[serde(rename = "shuffle")]
    Shuffle,
    #[serde(rename = "ended")]
    Ended,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidMusicReadyRequete {
    pub ready: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidMusicOptionsRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<PutVoiceCidMusicOptionsRequeteRepeat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PutVoiceCidMusicOptionsRequeteRepeat {
    #[serde(rename = "")]
    Vide,
    #[serde(rename = "one")]
    One,
    #[serde(rename = "all")]
    All,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidMusicQueueRequete {
    pub video: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidMusicQueueRequete {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidCinemaRequete {
    pub video: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidCinemaActionRequete {
    pub action: PostVoiceCidCinemaActionRequeteAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostVoiceCidCinemaActionRequeteAction {
    #[serde(rename = "play")]
    Play,
    #[serde(rename = "pause")]
    Pause,
    #[serde(rename = "seek")]
    Seek,
    #[serde(rename = "rate")]
    Rate,
    #[serde(rename = "previous")]
    Previous,
    #[serde(rename = "shuffle")]
    Shuffle,
    #[serde(rename = "ended")]
    Ended,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidCinemaReadyRequete {
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidCinemaOptionsRequete {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<PutVoiceCidCinemaOptionsRequeteRepeat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PutVoiceCidCinemaOptionsRequeteRepeat {
    #[serde(rename = "")]
    Vide,
    #[serde(rename = "one")]
    One,
    #[serde(rename = "all")]
    All,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostVoiceCidCinemaQueueRequete {
    pub video: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidCinemaQueueRequete {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutChannelsCidVideosPostsMidWatchedRequete {
    pub watched: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostChannelsCidVideosListsRequete {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collaborative: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutChannelsCidVideosListsLidRequete {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collaborative: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutChannelsCidVideosListsLidItemsRequete {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostChannelsCidVideosSessionsRequete {
    pub title: String,
    pub starts_at: String,
    pub voice_channel_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remind_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PutChannelsCidVideosSessionsSidRsvpRequete {
    pub answer: PutChannelsCidVideosSessionsSidRsvpRequeteAnswer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PutChannelsCidVideosSessionsSidRsvpRequeteAnswer {
    #[serde(rename = "yes")]
    Yes,
    #[serde(rename = "maybe")]
    Maybe,
    #[serde(rename = "no")]
    No,
    #[serde(other)]
    Inconnu,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PutVoiceCidParticipantsUidModRequete {
    pub mic_off: bool,
    pub sound_off: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PostWsTicketReponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
}
