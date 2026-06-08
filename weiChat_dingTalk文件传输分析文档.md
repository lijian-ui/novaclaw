# 微信 & 钉钉 文件传输与 LLM 分发分析文档

> 分析日期：2026-06-04
> 分析目标：详细梳理微信 iLink Bot SDK 和钉钉 Stream SDK / OpenClaw Connector 中，多模态文件（图片、视频、音频、文件）的接收、处理、发送给 LLM 的完整链路。

---

## 目录

1. [微信文件处理链路](#1-微信文件处理链路)
   - [1.1 入站（接收用户消息）](#11-入站接收用户消息)
     - [1.1.1 消息结构定义](#111-消息结构定义)
     - [1.1.2 入站消息处理入口](#112-入站消息处理入口)
     - [1.1.3 媒体文件下载与解密](#113-媒体文件下载与解密)
     - [1.1.4 CDN 下载与 AES 解密](#114-cdn-下载与-aes-解密)
     - [1.1.5 构建 LLM 上下文](#115-构建-llm-上下文)
     - [1.1.6 视频处理（入站）](#116-视频处理入站)
     - [1.1.7 语音处理（入站 + SILK→WAV 转码）](#117-语音处理入站--silkwav-转码)
     - [1.1.8 媒体优先级总结](#118-媒体优先级总结)
     - [1.1.9 媒体转发给 LLM 的上下文构建](#119-媒体转发给-llm-的上下文构建)
   - [1.2 出站（发送消息给用户）](#12-出站发送消息给用户)
     - [1.2.1 文件上传到微信 CDN](#121-文件上传到微信-cdn)
     - [1.2.2 发送媒体消息](#122-发送媒体消息)
     - [1.2.3 媒体类型路由](#123-媒体类型路由)
   - [1.3 发送给 LLM 的方式](#13-发送给-llm-的方式)
2. [钉钉文件处理链路](#2-钉钉文件处理链路)
   - [2.1 Rust Stream SDK 层](#21-rust-stream-sdk-层)
     - [2.1.1 消息类型定义](#211-消息类型定义)
     - [2.1.2 文件下载实现](#212-文件下载实现)
     - [2.1.3 文件上传实现](#213-文件上传实现)
   - [2.2 OpenClaw Connector 层](#22-openclaw-connector-层)
     - [2.2.1 消息内容提取](#221-消息内容提取)
     - [2.2.2 图片下载](#222-图片下载)
     - [2.2.3 文件下载与内容解析](#223-文件下载与内容解析)
     - [2.2.4 视频处理（入站）](#224-视频处理入站)
     - [2.2.5 音频处理（入站）](#225-音频处理入站)
     - [2.2.6 大文件分块上传](#226-大文件分块上传)
     - [2.2.7 视频/音频入站处理总结](#227-视频音频入站处理总结)
   - [2.3 发送给 LLM 的方式](#23-发送给-llm-的方式)
     - [2.3.1 图片 → Markdown 图片语法](#231-图片--markdown-图片语法)
     - [2.3.2 文件 → 文本内容嵌入](#232-文件--文本内容嵌入)
     - [2.3.3 最终分发](#233-最终分发)
3. [核心差异对比表](#3-核心差异对比表)
4. [总结](#4-总结)

---

## 1. 微信文件处理链路

### 1.1 入站（接收用户消息）

微信 iLink Bot API 的入站消息通过长轮询 `getupdates` 接口获取，消息体中的 `item_list` 数组包含多种类型的 `MessageItem`。

#### 1.1.1 消息结构定义

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/api/types.ts`

```typescript
// MessageItemType 枚举
export enum MessageItemType {
  TEXT = 1,
  IMAGE = 2,
  VIDEO = 3,
  FILE = 4,
  VOICE = 5,
}

// 媒体 CDN 引用结构
export interface CDNMedia {
  encrypt_query_param?: string;  // CDN 下载加密参数
  aes_key?: string;             // AES-128-ECB 密钥（base64 编码）
  full_url?: string;            // 完整 CDN URL（可选）
  encrypt_type?: number;        // 加密类型
}

// 各消息类型的结构
export interface ImageItem {
  media?: CDNMedia;
  mid_size?: number;            // 密文文件大小
  hd_size?: number;
  aeskey?: string;              // 部分版本密钥在此
}

export interface VideoItem {
  media?: CDNMedia;
  video_size?: number;
}

export interface FileItem {
  media?: CDNMedia;
  file_name?: string;
  len?: string;                 // 明文文件大小
}

export interface VoiceItem {
  media?: CDNMedia;
  text?: string;                // 语音转文字结果
}
```

#### 1.1.2 入站消息处理入口

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/messaging/process-message.ts`

核心流程：从 `item_list` 中按优先级（IMAGE > VIDEO > FILE > VOICE）查找第一个可下载的媒体项，然后调用 `downloadMediaFromItem()`。

```typescript
// 查找第一个可下载的媒体项（优先级：IMAGE > VIDEO > FILE > VOICE）
const mainMediaItem =
  full.item_list?.find(
    (i) => i.type === MessageItemType.IMAGE && hasDownloadableMedia(i.image_item?.media),
  ) ??
  full.item_list?.find(
    (i) => i.type === MessageItemType.VIDEO && hasDownloadableMedia(i.video_item?.media),
  ) ??
  full.item_list?.find(
    (i) => i.type === MessageItemType.FILE && hasDownloadableMedia(i.file_item?.media),
  ) ??
  full.item_list?.find(
    (i) =>
      i.type === MessageItemType.VOICE &&
      hasDownloadableMedia(i.voice_item?.media) &&
      !i.voice_item?.text,
  );

// 下载媒体文件
const mediaItem = mainMediaItem ?? refMediaItem;
if (mediaItem) {
  const downloaded = await downloadMediaFromItem(mediaItem, {
    cdnBaseUrl: deps.cdnBaseUrl,
    saveMedia: deps.channelRuntime.media.saveMediaBuffer,
    log: deps.log,
    errLog: deps.errLog,
    label,
  });
  Object.assign(mediaOpts, downloaded);
}
```

#### 1.1.3 媒体文件下载与解密

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/media/media-download.ts`

核心函数 `downloadMediaFromItem()` 根据媒体类型（IMAGE、VOICE、FILE、VIDEO）分别处理：

```typescript
export async function downloadMediaFromItem(item, deps): Promise<WeixinInboundMediaOpts> {
  if (item.type === MessageItemType.IMAGE) {
    // 图片：下载 CDN → AES-128-ECB 解密 → 保存到本地
    const buf = await downloadAndDecryptBuffer(
      img.media.encrypt_query_param,
      aesKeyBase64,
      cdnBaseUrl,
      label,
      img.media.full_url,
    );
    const saved = await saveMedia(buf, undefined, "inbound", WEIXIN_MEDIA_MAX_BYTES);
    result.decryptedPicPath = saved.path;  // 本地文件路径
  } else if (item.type === MessageItemType.FILE) {
    // 文件：同上，额外保留文件名
    const buf = await downloadAndDecryptBuffer(...);
    const saved = await saveMedia(buf, mime, "inbound", WEIXIN_MEDIA_MAX_BYTES, fileItem.file_name);
    result.decryptedFilePath = saved.path;
  } else if (item.type === MessageItemType.VIDEO) {
    // 视频：同上
    const buf = await downloadAndDecryptBuffer(...);
    const saved = await saveMedia(buf, "video/mp4", "inbound", WEIXIN_MEDIA_MAX_BYTES);
    result.decryptedVideoPath = saved.path;
  } else if (item.type === MessageItemType.VOICE) {
    // 语音：下载解密后，尝试 SILK→WAV 转码
    const silkBuf = await downloadAndDecryptBuffer(...);
    const wavBuf = await silkToWav(silkBuf);
    // 保存为 WAV 或原始 SILK
  }
}
```

#### 1.1.4 CDN 下载与 AES 解密

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/cdn/pic-decrypt.ts`

```typescript
export async function downloadAndDecryptBuffer(
  encryptedQueryParam: string,
  aesKeyBase64: string,
  cdnBaseUrl: string,
  label: string,
  fullUrl?: string,
): Promise<Buffer> {
  const key = parseAesKey(aesKeyBase64, label);  // 解析 AES 密钥
  const url = fullUrl || buildCdnDownloadUrl(encryptedQueryParam, cdnBaseUrl);
  const encrypted = await fetchCdnBytes(url, label);  // 从 CDN 下载密文
  const decrypted = decryptAesEcb(encrypted, key);    // AES-128-ECB 解密
  return decrypted;
}
```

#### 1.1.5 构建 LLM 上下文

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/messaging/inbound.ts`

解密后的媒体文件通过 `MediaPath`（本地文件路径）传递给 LLM：

```typescript
export function weixinMessageToMsgContext(
  msg: WeixinMessage,
  accountId: string,
  opts?: WeixinInboundMediaOpts,
): WeixinMsgContext {
  const ctx: WeixinMsgContext = {
    Body: bodyFromItemList(msg.item_list),  // 文本内容
    From: from_user_id,
    // ... 其他字段
  };

  // 优先传递本地文件路径，而非 CDN URL
  if (opts?.decryptedPicPath) {
    ctx.MediaPath = opts.decryptedPicPath;   // 本地图片路径
    ctx.MediaType = "image/*";
  } else if (opts?.decryptedVideoPath) {
    ctx.MediaPath = opts.decryptedVideoPath; // 本地视频路径
    ctx.MediaType = "video/mp4";
  } else if (opts?.decryptedFilePath) {
    ctx.MediaPath = opts.decryptedFilePath;  // 本地文件路径
    ctx.MediaType = opts.fileMediaType ?? "application/octet-stream";
  } else if (opts?.decryptedVoicePath) {
    ctx.MediaPath = opts.decryptedVoicePath; // 本地音频路径
    ctx.MediaType = opts.voiceMediaType ?? "audio/wav";
  }

  return ctx;
}
```

关键点：微信的入站媒体处理**不传递 CDN URL**，而是将加密文件**下载并解密到本地**后，传递 **`MediaPath`（本地文件路径）** 给 LLM。

#### 1.1.6 视频处理（入站）

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/media/media-download.ts`

微信视频消息（`MessageItemType.VIDEO = 3`）的处理与图片类似——从 CDN 下载、AES-128-ECB 解密、保存到本地：

```typescript
// media-download.ts (约第 125-145 行)
} else if (item.type === MessageItemType.VIDEO) {
    const videoItem = item.video_item;
    if ((!videoItem?.media?.encrypt_query_param && !videoItem?.media?.full_url) || !videoItem?.media?.aes_key)
      return result;
    try {
      const buf = await downloadAndDecryptBuffer(
        videoItem.media.encrypt_query_param ?? "",
        videoItem.media.aes_key,
        cdnBaseUrl,
        `${label} video`,
        videoItem.media.full_url,
      );
      const saved = await saveMedia(buf, "video/mp4", "inbound", WEIXIN_MEDIA_MAX_BYTES);
      result.decryptedVideoPath = saved.path;       // 本地视频路径
    } catch (err) { /* ... */ }
}
```

处理特点：
- 微信 `VideoItem` 结构中的媒体引用（`CDNMedia`）与其他类型共用
- 视频下载解密后统一保存为 `video/mp4` 格式
- 不进行视频元数据提取（无时长、分辨率分析），也不生成封面图
- 解密后的本地路径写入 `WeixinInboundMediaOpts.decryptedVideoPath`
- 视频文件上限：`WEIXIN_MEDIA_MAX_BYTES = 100MB`

#### 1.1.7 语音处理（入站）

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/media/media-download.ts`  
**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/media/silk-transcode.ts`

微信语音消息（`MessageItemType.VOICE = 5`）的处理包含一个关键步骤——**SILK → WAV 转码**：

```typescript
// media-download.ts (约第 65-100 行)
} else if (item.type === MessageItemType.VOICE) {
    const voice = item.voice_item;
    if ((!voice?.media?.encrypt_query_param && !voice?.media?.full_url) || !voice?.media?.aes_key)
      return result;
    try {
      const silkBuf = await downloadAndDecryptBuffer(/* ... */);
      // 关键：将 SILK 格式转码为 WAV
      const wavBuf = await silkToWav(silkBuf);
      if (wavBuf) {
        const saved = await saveMedia(wavBuf, "audio/wav", "inbound", WEIXIN_MEDIA_MAX_BYTES);
        result.decryptedVoicePath = saved.path;
        result.voiceMediaType = "audio/wav";
      } else {
        // 转码失败则保存原始 SILK
        const saved = await saveMedia(silkBuf, "audio/silk", "inbound", WEIXIN_MEDIA_MAX_BYTES);
        result.decryptedVoicePath = saved.path;
        result.voiceMediaType = "audio/silk";
      }
    } catch (err) { /* ... */ }
}
```

SILK → WAV 转码的具体实现：

```typescript
// silk-transcode.ts
const SILK_SAMPLE_RATE = 24_000;  // 微信语音采样率 24kHz

export async function silkToWav(silkBuf: Buffer): Promise<Buffer | null> {
  try {
    const { decode } = await import("silk-wasm");   // 使用 silk-wasm 库解码
    const result = await decode(silkBuf, SILK_SAMPLE_RATE);  // 输出 PCM s16le
    const wav = pcmBytesToWav(result.data, SILK_SAMPLE_RATE); // 封装 WAV 头
    return wav;
  } catch (err) {
    logger.warn(`silkToWav: transcode failed, will use raw silk err=${String(err)}`);
    return null;  // 转码失败返回 null，调用方保留原始 SILK
  }
}
```

处理特点：
- 微信语音使用 **SILK** 编码（Skype 开发的语音编码格式，压缩率高、适合网络传输）
- 下载解密后通过 `silk-wasm`（WebAssembly）解码为 PCM s16le 格式
- 解码后的 PCM 数据加上 44 字节 WAV 文件头（RIFF/WAVE/fmt/data chunk），封装为标准 WAV 文件
- **WAV 格式参数**：单声道、16 位有符号小端、24kHz 采样率
- 若 `silk-wasm` 不可用或解码失败，退化为保存原始 `.silk` 文件
- 语音消息若有 `voice_item.text` 字段（服务端语音转文字结果），则 **不会触发下载**，直接使用文字内容（见 `process-message.ts` 查找逻辑中 `!i.voice_item?.text` 的条件）

#### 1.1.8 微信入站媒体优先级总结

`process-message.ts` 中按以下优先级查找第一个可下载的媒体项：

1. **IMAGE**（图片）— 最高优先级
2. **VIDEO**（视频）
3. **FILE**（文件）
4. **VOICE**（语音）— 最低优先级，且要求 `!voice_item?.text`（无语音转文字时才下载）

当主 `item_list` 中没有可下载的媒体时，会回退检查引用消息（`ref_msg.message_item`）中的媒体项。

#### 1.1.9 媒体转发给 LLM 的上下文构建

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/messaging/inbound.ts`

所有媒体类型最终通过 `weixinMessageToMsgContext()` 构建为统一的 `WeixinMsgContext`，传递给 LLM：

```typescript
// inbound.ts — 媒体优先顺序：图片 > 视频 > 文件 > 语音
if (opts?.decryptedPicPath) {
    ctx.MediaPath = opts.decryptedPicPath;       // 本地图片路径
    ctx.MediaType = "image/*";
} else if (opts?.decryptedVideoPath) {
    ctx.MediaPath = opts.decryptedVideoPath;     // 本地视频路径
    ctx.MediaType = "video/mp4";
} else if (opts?.decryptedFilePath) {
    ctx.MediaPath = opts.decryptedFilePath;      // 本地文件路径
    ctx.MediaType = opts.fileMediaType ?? "application/octet-stream";
} else if (opts?.decryptedVoicePath) {
    ctx.MediaPath = opts.decryptedVoicePath;     // 本地音频路径（WAV 或原始 SILK）
    ctx.MediaType = opts.voiceMediaType ?? "audio/wav";
}
```

关键点：媒体优先级在 `weixinMessageToMsgContext` 中再次体现，**同一条消息只传递一种媒体**给 LLM，优先顺序为 `图片 > 视频 > 文件 > 语音`。

---

### 1.2 出站（发送消息给用户）

#### 1.2.1 文件上传到微信 CDN

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/cdn/upload.ts`

文件发送前需先上传到微信 CDN，流程如下：

```typescript
async function uploadMediaToCdn(params): Promise<UploadedFileInfo> {
  // 1. 读取本地文件
  const plaintext = await fs.readFile(filePath);
  const rawsize = plaintext.length;
  
  // 2. MD5 哈希
  const rawfilemd5 = crypto.createHash("md5").update(plaintext).digest("hex");
  
  // 3. 计算 AES 填充后的密文大小
  const filesize = aesEcbPaddedSize(rawsize);
  
  // 4. 生成随机 AES 密钥（16 字节）
  const aeskey = crypto.randomBytes(16);
  
  // 5. 调用 getUploadUrl 获取上传地址
  const uploadUrlResp = await getUploadUrl({
    filekey, media_type, to_user_id,
    rawsize, rawfilemd5, filesize, aeskey,
  });
  
  // 6. AES-128-ECB 加密并上传到 CDN
  const { downloadParam } = await uploadBufferToCdn({
    buf: plaintext,
    uploadFullUrl, uploadParam, filekey, cdnBaseUrl, aeskey,
  });
  
  return {
    filekey,
    downloadEncryptedQueryParam: downloadParam, // CDN 下载参数
    aeskey: aeskey.toString("hex"),              // AES 密钥（hex）
    fileSize: rawsize,
    fileSizeCiphertext: filesize,
  };
}
```

#### 1.2.2 发送媒体消息

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/messaging/send.ts`

```typescript
// 发送图片消息
export async function sendImageMessageWeixin(params) {
  const imageItem: MessageItem = {
    type: MessageItemType.IMAGE,
    image_item: {
      media: {
        encrypt_query_param: uploaded.downloadEncryptedQueryParam,
        aes_key: Buffer.from(uploaded.aeskey).toString("base64"),
        encrypt_type: 1,
      },
      mid_size: uploaded.fileSizeCiphertext,
    },
  };
  return sendMediaItems({ to, text, mediaItem: imageItem, opts, label });
}

// 发送文件消息
export async function sendFileMessageWeixin(params) {
  const fileItem: MessageItem = {
    type: MessageItemType.FILE,
    file_item: {
      media: {
        encrypt_query_param: uploaded.downloadEncryptedQueryParam,
        aes_key: Buffer.from(uploaded.aeskey).toString("base64"),
        encrypt_type: 1,
      },
      file_name: fileName,
      len: String(uploaded.fileSize),
    },
  };
  return sendMediaItems({ to, text, mediaItem: fileItem, opts, label });
}
```

#### 1.2.3 媒体类型路由

**文件：** `tencent-weixin-openclaw-weixin-2.4.3/package/src/messaging/send-media.ts`

```typescript
export async function sendWeixinMediaFile(params) {
  const mime = getMimeFromFilename(filePath);
  
  if (mime.startsWith("video/")) {
    // 视频：uploadVideoToWeixin + sendVideoMessageWeixin
  } else if (mime.startsWith("image/")) {
    // 图片：uploadFileToWeixin + sendImageMessageWeixin
  } else {
    // 文件附件：uploadFileAttachmentToWeixin + sendFileMessageWeixin
  }
}
```

### 1.3 发送给 LLM 的方式

微信 SDK 通过 OpenClaw 框架的 `dispatchReplyFromConfig` 将消息分发给 LLM，其核心机制是：

1. **`process-message.ts`** → 从 `item_list` 提取媒体 → 调用 `downloadMediaFromItem()` 下载解密 → 调用 `weixinMessageToMsgContext()` 构建 `WeixinMsgContext`
2. **`WeixinMsgContext`** 中包含 `MediaPath`（本地文件路径）和 `MediaType`
3. **`dispatchReplyFromConfig()`** 将 `ctx` 传入 OpenClaw 核心管道
4. OpenClaw 核心管道将 `MediaPath` 指向的本地文件传给 LLM 处理

LLM 返回结果后，通过前面描述的出站流程（上传 CDN → 发送消息）将媒体回复给用户。

---

## 2. 钉钉文件处理链路

钉钉的文件处理分为两层：
- **底层 Rust Stream SDK**：提供原始的消息类型定义和资源下载能力
- **上层 OpenClaw Connector (TypeScript)**：实现完整的文件下载、解析、LLM 分发逻辑

### 2.1 Rust Stream SDK 层

#### 2.1.1 消息类型定义

**文件：** `dingtalk-stream-sdk-rust/src/frames/down_message/callback_message.rs`

```rust
/// 消息载荷枚举，按 msgtype 字段分发
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msgtype")]
pub enum MessagePayload {
    #[serde(rename = "text")]
    Text { text: PayloadText },
    #[serde(rename = "picture")]
    Picture { content: PayloadPicture },
    #[serde(rename = "video")]
    Video { content: PayloadVideo },
    #[serde(rename = "audio")]
    Audio { content: PayloadAudio },
    #[serde(rename = "file")]
    File { content: PayloadFile },
    #[serde(rename = "richText")]
    RichText { content: PayloadRichText },
}

/// 图片：通过 downloadCode 引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadPicture {
    #[serde(rename = "downloadCode")]
    pub download_code: String,
    #[serde(rename = "pictureDownloadCode")]
    pub picture_download_code: String,
}

/// 文件：通过 downloadCode + fileId 引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadFile {
    #[serde(rename = "downloadCode")]
    pub download_code: String,
    #[serde(rename = "fileId")]
    pub file_id: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "spaceId")]
    pub space_id: String,
}

/// 视频
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadVideo {
    #[serde(rename = "downloadCode")]
    pub download_code: String,
    #[serde(rename = "duration")]
    pub duration: String,
    #[serde(rename = "videoType")]
    pub video_type: String,
}

/// 音频
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAudio {
    #[serde(rename = "downloadCode")]
    pub download_code: String,
    #[serde(rename = "recognition")]
    pub recognition: String,
}
```

所有媒体类型统一通过 `downloadCode` 字段引用文件。

#### 2.1.2 文件下载实现

**文件：** `dingtalk-stream-sdk-rust/src/client/stream_/download_resources.rs`

```rust
/// DingtalkResource trait：定义了统一的资源下载接口
#[async_trait]
pub trait DingtalkResource {
    type T;
    async fn fetch(
        &self,
        dingtalk: &DingTalkStream,
        save_to: PathBuf,
    ) -> crate::Result<(PathBuf, Self::T)>;
}

/// 文件下载实现
#[async_trait]
impl DingtalkResource for PayloadFile {
    type T = Vec<u8>;
    async fn fetch(&self, dingtalk: &DingTalkStream, save_to_dir: PathBuf) -> crate::Result<(PathBuf, Self::T)> {
        // 1. 检查缓存文件是否存在（基于 downloadCode 的 MD5）
        let filepath = save_to_dir.join(format!("{}_{}", md5_hex(&self.download_code), self.file_name));
        if filepath.exists() { return Ok((filepath, fs::read(&filepath).await?)); }
        
        // 2. 通过 downloadCode 换取真实下载 URL
        let download_url = fetch_download_url(dingtalk, &self.download_code).await?;
        
        // 3. 直接下载文件
        let bytes = dingtalk.http_client.get(download_url).send().await?.bytes().await?;
        fs::write(&filepath, bytes.as_ref()).await?;
        
        Ok((filepath, bytes.to_vec()))
    }
}

/// 通过 downloadCode 换取下载 URL
async fn fetch_download_url(dingtalk: &DingTalkStream, download_code: &str) -> crate::Result<Url> {
    let access_token = dingtalk.get_access_token().await?;
    let response = dingtalk.http_client
        .post("https://api.dingtalk.com/v1.0/robot/messageFiles/download")
        .header("x-acs-dingtalk-access-token", access_token)
        .json(&json!({
            "robotCode": &dingtalk.credential.client_id,
            "downloadCode": download_code,
        }))
        .send().await?;
    let json = response.json::<serde_json::Value>().await?;
    let download_url = json.get("downloadUrl").and_then(|it| it.as_str());
    Ok(download_url?.try_into()?)
}
```

#### 2.1.3 文件上传实现

**文件：** `dingtalk-stream-sdk-rust/src/client/stream_/upload_resources.rs`

```rust
/// DingTalkMedia trait：统一的上传接口
#[async_trait]
pub trait DingTalkMedia {
    async fn upload(&self, dingtalk: &DingTalkStream) -> crate::Result<MediaUploadResult>;
}

/// 上传实现
impl DingTalkMedia_ {
    async fn upload_(&self, dingtalk: &DingTalkStream) -> crate::Result<MediaUploadResult> {
        let access_token = dingtalk.get_access_token().await?;
        let bytes = self.as_bytes().await?;  // 支持 Bytes / Filepath / Url 三种来源
        let form = reqwest::multipart::Form::new()
            .text("type", self.type_().to_string())
            .part("media", Part::bytes(bytes).file_name(filename));
        let result = dingtalk.http_client
            .post("https://oapi.dingtalk.com/media/upload")
            .query(&[("access_token", access_token)])
            .multipart(form)
            .send().await?
            .json::<MediaUploadResult>().await?;
        Ok(result)
    }
}
```

### 2.2 OpenClaw Connector 层

#### 2.2.1 消息内容提取

**文件：** `dingtalk-openclaw-connector/src/core/message-handler.ts`

`extractMessageContent()` 函数处理所有钉钉消息类型，提取文本和媒体信息：

```typescript
export function extractMessageContent(data: any): ExtractedMessage {
  const msgtype = data.msgtype || 'text';
  switch (msgtype) {
    case 'picture': {
      const content = resolveContent(data);
      const downloadCode = content?.downloadCode || '';
      return {
        text: '[图片]',
        messageType: 'picture',
        imageUrls: [],
        downloadCodes: [downloadCode],  // 图片的 downloadCode
        fileNames: [],
        // ...
      };
    }
    case 'file': {
      const content = resolveContent(data);
      const fileName = content?.fileName || '文件';
      const downloadCode = content?.downloadCode || '';
      return {
        text: `[文件: ${fileName}]`,
        messageType: 'file',
        downloadCodes: [downloadCode],   // 文件的 downloadCode
        fileNames: [fileName],           // 文件名
        // ...
      };
    }
    case 'richText': {
      // 富文本：遍历 richTextList，提取文本和媒体
      for (const item of richList) {
        if (item.text) textParts.push(item.text);
        if (item.downloadCode) {
          // 根据 type 区分图片/视频/音频/文件
          downloadCodes.push(item.downloadCode);
          fileNames.push(item.fileName || '文件');
        }
      }
    }
  }
}
```

#### 2.2.2 图片下载

**文件：** `dingtalk-openclaw-connector/src/core/message-handler.ts`

图片通过 `downloadCode` 换取下载 URL 后，下载到本地：

```typescript
// 通过 downloadCode 下载图片
export async function downloadMediaByCode(
  downloadCode: string,
  config: DingtalkConfig,
  agentWorkspaceDir: string,
  log?: any,
): Promise<string | null> {
  // 1. 获取 accessToken
  const token = await getAccessToken(config);
  
  // 2. 通过 downloadCode 换取下载 URL
  const resp = await dingtalkHttp.post(
    `${DINGTALK_API}/v1.0/robot/messageFiles/download`,
    { downloadCode, robotCode: String(config.clientId) },
    { headers: { 'x-acs-dingtalk-access-token': token } },
  );
  const downloadUrl = resp.data?.downloadUrl;
  
  // 3. 下载到本地
  return downloadImageToFile(downloadUrl, agentWorkspaceDir, log);
}

// 实际文件下载
export async function downloadImageToFile(
  downloadUrl: string,
  agentWorkspaceDir: string,
  log?: any,
): Promise<string | null> {
  const resp = await dingtalkHttp.get(downloadUrl, { responseType: 'arraybuffer' });
  const buffer = Buffer.from(resp.data);
  const mediaDir = path.join(agentWorkspaceDir, 'media', 'inbound');
  const tmpFile = path.join(mediaDir, `openclaw-media-${Date.now()}-${random}.${ext}`);
  fs.writeFileSync(tmpFile, buffer);
  return tmpFile;
}
```

#### 2.2.3 文件下载与内容解析

**文件：** `dingtalk-openclaw-connector/src/core/message-handler.ts`

钉钉 Connector 对文件做了更精细的处理——不仅下载文件，还会尝试解析文件内容：

```typescript
// 下载文件到本地
export async function downloadFileToLocal(
  downloadUrl: string,
  fileName: string,
  agentWorkspaceDir: string,
  log?: any,
): Promise<string | null> {
  const resp = await dingtalkHttp.get(downloadUrl, { responseType: 'arraybuffer' });
  const buffer = Buffer.from(resp.data);
  const localPath = path.join(mediaDir, safeFileName);
  fs.writeFileSync(localPath, buffer);
  return localPath;
}

// 根据扩展名解析文件内容
async function parseFileContent(filePath: string, fileName: string, log?: any) {
  const ext = path.extname(fileName).toLowerCase();
  if (['.docx', '.doc'].includes(ext)) return parseDocxFile(filePath, log);
  if (ext === '.pdf') return parsePdfFile(filePath, log);
  if (['.txt', '.md', '.json', ...].includes(ext)) return readTextFile(filePath, log);
  return { content: null, type: 'binary' };  // 二进制文件不解析
}
```

#### 2.2.4 视频处理（入站）

钉钉视频消息的入站处理涉及 Rust SDK 层的下载和 Connector 层的消息提取。

**Rust SDK 层 — 视频下载：**

**文件：** `dingtalk-stream-sdk-rust/src/client/stream_/download_resources.rs`

```rust
#[async_trait]
impl DingtalkResource for PayloadVideo {
    type T = Vec<u8>;
    async fn fetch(&self, dingtalk: &DingTalkStream, save_to_dir: PathBuf) -> crate::Result<(PathBuf, Self::T)> {
        let filepath = save_to_dir.join(format!(
            "{}.{}",
            format!("{:x}", md5::compute(&self.download_code)),  // MD5(downloadCode) 作为文件名
            self.video_type                                       // 使用 video_type 作为扩展名
        ));
        // 缓存检查：文件已存在则直接读取
        if filepath.exists() {
            let bytes = tokio::fs::read(&filepath).await?;
            return Ok((filepath, bytes));
        }
        // 通过 downloadCode 换取下载 URL
        let download_url = fetch_download_url(dingtalk, &self.download_code).await?;
        // 直接下载（无加密）
        let bytes = dingtalk.http_client.get(download_url).send().await?.bytes().await?;
        tokio::fs::write(&filepath, bytes.as_ref()).await?;
        Ok((filepath, bytes.to_vec()))
    }
}
```

视频结构体定义：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadVideo {
    #[serde(rename = "downloadCode")]
    pub download_code: String,      // 下载码
    #[serde(rename = "duration")]
    pub duration: String,           // 视频时长（秒）
    #[serde(rename = "videoType")]
    pub video_type: String,         // 视频格式（如 mp4）
}
```

**Connector 层 — 视频消息提取：**

**文件：** `dingtalk-openclaw-connector/src/core/message-handler.ts`

```typescript
// extractMessageContent — 视频消息
case 'video':
    bodyText = '[视频]';  // 纯视频消息的文本仅为占位符
    break;

// resolveContent — 视频消息
case 'video': {
    const code = contentObj?.downloadCode;
    if (code) {
        downloadCodes.push(code);
        fileNames.push(contentObj?.fileName || 'video.mp4');
    }
    break;
}

// extractRichTextItems — 富文本中的视频
if (itemType === 'video') {
    downloadCodes.push(item.downloadCode);
    fileNames.push(item.fileName || 'video.mp4');
}
```

**Connector 层 — 视频文件附件处理：**

当 `downloadCodes` 中有对应的 `fileName` 时，视频按文件附件流程处理：

```typescript
// message-handler.ts (约第 1282 行)
if (['.mp4', '.avi', '.mov', '.mkv', '.flv', '.wmv', '.webm'].includes(ext)) {
    fileType = '视频';
}
```

视频文件按**二进制文件**处理，不解析内容，只保存本地路径和提供下载链接：

```typescript
// 二进制文件的处理结果
fileContentParts.push(
    `📎 **${fileType}**: ${fileName}\n` +
    `💾 已保存到本地: ${localPath}\n` +
    `🔗 [点击下载](${downloadUrl})`
);
```

**视频出站处理（服务端发送视频给用户）：**

**文件：** `dingtalk-openclaw-connector/src/services/media/video.ts`

Connector 支持对 LLM 回复中的视频进行元数据提取和上传：

```typescript
// 1. 提取视频元数据（时长、分辨率）
export async function extractVideoMetadata(filePath: string, log?: any) {
    const ffmpeg = require('fluent-ffmpeg');
    return new Promise((resolve) => {
        ffmpeg.ffprobe(filePath, (err: any, metadata: any) => {
            const duration = metadata.format?.duration ? Math.floor(parseFloat(metadata.format.duration)) : 0;
            const videoStream = metadata.streams?.find((s: any) => s.codec_type === 'video');
            resolve({ duration, width: videoStream?.width || 0, height: videoStream?.height || 0 });
        });
    });
}

// 2. 生成视频封面图（第 1 秒截图）
export async function extractVideoThumbnail(videoPath: string, outputPath: string, log?: any) {
    ffmpeg(videoPath)
        .screenshots({
            count: 1,
            folder: path.dirname(outputPath),
            filename: path.basename(outputPath),
            timemarks: ['1'],       // 第 1 秒截图
            size: '?x360',          // 高度 360px，宽度自适应
        });
    // ...
}

// 3. 上传视频并通过标记处理出站消息
export async function processVideoMarkers(content, sessionWebhook, config, oapiToken, log) {
    // 检测 [DINGTALK_VIDEO]{...path:...}[/DINGTALK_VIDEO] 标记
    const matches = [...content.matchAll(VIDEO_MARKER_PATTERN)];
    for (const match of matches) {
        const absPath = toLocalPath(videoData.path);
        // 上传视频到钉钉 OAPI
        const mediaId = await uploadMediaToDingTalk(absPath, 'video', oapiToken, 20 * 1024 * 1024, log);
        result = result.replace(full, mediaId ? `[视频已上传：${mediaId}]` : '⚠️ 视频上传失败');
    }
}
```

#### 2.2.5 音频处理（入站）

钉钉音频消息的处理与视频类似，但有额外的语音识别文本支持。

**Rust SDK 层 — 音频下载：**

**文件：** `dingtalk-stream-sdk-rust/src/client/stream_/download_resources.rs`

```rust
#[async_trait]
impl DingtalkResource for PayloadAudio {
    type T = Vec<u8>;
    async fn fetch(&self, dingtalk: &DingTalkStream, save_to_dir: PathBuf) -> crate::Result<(PathBuf, Self::T)> {
        let filepath = save_to_dir.join(format!(
            "{}.mp3",                                             // 统一保存为 .mp3 格式
            format!("{:x}", md5::compute(&self.download_code)),   // MD5(downloadCode) 作为文件名
        ));
        if filepath.exists() {
            let bytes = tokio::fs::read(&filepath).await?;
            return Ok((filepath, bytes));
        }
        let download_url = fetch_download_url(dingtalk, &self.download_code).await?;
        let bytes = dingtalk.http_client.get(download_url).send().await?.bytes().await?;
        tokio::fs::write(&filepath, bytes.as_ref()).await?;
        Ok((filepath, bytes.to_vec()))
    }
}
```

音频结构体定义：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAudio {
    #[serde(rename = "downloadCode")]
    pub download_code: String,      // 下载码
    #[serde(rename = "recognition")]
    pub recognition: String,        // 语音转文字结果（服务端自动识别）
}
```

**Connector 层 — 音频消息提取：**

**文件：** `dingtalk-openclaw-connector/src/core/message-handler.ts`

```typescript
// extractMessageContent — 音频消息（关键：使用语音识别文本作为 body）
case 'audio': {
    const content = resolveContent(data);
    const recognition =
        content?.recognition ||
        data?.audio?.recognition ||
        '[语音消息]';                         // 无识别结果则用占位符
    const audioDownloadCode = content?.downloadCode || '';
    const audioFileName = content?.fileName || 'audio.amr';
    return {
        text: recognition,                     // ★ 语音识别文本作为消息正文
        messageType: 'audio',
        downloadCodes: audioDownloadCode ? [audioDownloadCode] : [],
        fileNames: audioFileName ? [audioFileName] : [],
    };
}

// -----------------------------------------------------------------------

// extractRichTextItems — 富文本中的音频
if (itemType === 'audio') {
    downloadCodes.push(item.downloadCode);
    fileNames.push(item.fileName || 'audio.amr');
}
```

**音频文件附件处理（按二进制文件 + 语音识别文本）：**

```typescript
// message-handler.ts (约第 1284 行)
if (['.mp3', '.wav', '.aac', '.ogg', '.m4a', '.flac', '.wma'].includes(ext)) {
    fileType = '音频';
}

// 特殊处理：音频文件附加上语音识别文本
if (fileType === '音频' && content.text && content.text !== '[语音消息]') {
    fileContentParts.push(
        `🎤 **${fileType}**: ${fileName}\n` +
        `📝 语音识别: ${content.text}\n` +       // 语音识别文本注入上下文
        `💾 已保存到本地: ${localPath}\n` +
        `🔗 [点击下载](${downloadUrl})`
    );
} else {
    // 普通二进制文件处理
    fileContentParts.push(
        `📎 **${fileType}**: ${fileName}\n` +
        `💾 已保存到本地: ${localPath}\n` +
        `🔗 [点击下载](${downloadUrl})`
    );
}
```

**音频出站处理（服务端发送音频给用户）：**

**文件：** `dingtalk-openclaw-connector/src/services/media/audio.ts`

```typescript
export async function processAudioMarkers(content, sessionWebhook, config, oapiToken, log) {
    // 检测 [DINGTALK_AUDIO]{...path:...}[/DINGTALK_AUDIO] 标记
    const matches = [...content.matchAll(AUDIO_MARKER_PATTERN)];
    for (const match of matches) {
        const absPath = toLocalPath(audioData.path);
        if (!fs.existsSync(absPath)) { /* 文件不存在处理 */ }
        // 上传音频到钉钉 OAPI（类型为 voice）
        const uploadResult = await uploadMediaToDingTalk(absPath, 'voice', oapiToken, 20 * 1024 * 1024, log);
        result = result.replace(full, uploadResult ? `[音频已上传：${uploadResult}]` : '⚠️ 音频上传失败');
    }
    return result.trim();
}
```

#### 2.2.6 视频/音频大文件分块上传

**文件：** `dingtalk-openclaw-connector/src/services/media/chunk-upload.ts`

钉钉对超过 **20MB** 的视频和文件使用分块上传机制：

```typescript
export const CHUNK_CONFIG = {
    MIN_CHUNK_SIZE: 100 * 1024,       // 最小分块 100KB
    MAX_CHUNK_SIZE: 8 * 1024 * 1024,  // 最大分块 8MB
    DEFAULT_CHUNK_SIZE: 5 * 1024 * 1024, // 默认分块 5MB
    SIZE_THRESHOLD: 20 * 1024 * 1024, // 超过 20MB 触发分块
};

// 分块上传三步流程：
// 步骤一：enableUploadTransaction() — 开启上传事务，获取 upload_id
// 步骤二：uploadFileBlock() — 分块上传（按 chunkSize 切片，逐块上传）
// 步骤三：submitUploadTransaction() — 提交事务，获取 download_code
```

#### 2.2.7 视频/音频入站处理总结

| 步骤 | 视频 | 音频 |
|------|------|------|
| **消息提取** | `bodyText = '[视频]'`，提取 `downloadCode` + `fileName` | `bodyText = recognition || '[语音消息]'`，提取 `downloadCode` |
| **Rust SDK 下载** | `PayloadVideo.fetch()` → 保存为 `{md5}.{video_type}` | `PayloadAudio.fetch()` → 保存为 `{md5}.mp3` |
| **Connector 下载** | 通过 `getFileDownloadUrl` + `downloadFileToLocal` 下载 | 同视频流程 |
| **内容解析** | 二进制，不解析内容 | 二进制，不解析内容，但注入 `recognition` 文本 |
| **LLM 上下文** | 文件路径 + 下载链接 | 语音识别文本 + 文件路径 + 下载链接 |
| **出站上传** | `uploadMediaToDingTalk(type='video')` | `uploadMediaToDingTalk(type='voice')` |
| **大文件支持** | 支持分块上传（>20MB） | 不支持分块（通常文件较小） |

---

### 2.3 发送给 LLM 的方式

钉钉 OpenClaw Connector 通过以下方式将媒体文件发送给 LLM：

#### 2.3.1 图片 → Markdown 图片语法

下载后的图片通过 `![image](file://{localPath})` 格式注入到消息文本中：

```typescript
// file: dingtalk-openclaw-connector/src/core/message-handler.ts (约第 1450 行)
let finalContent = userContent;
if (imageLocalPaths.length > 0) {
  const imageMarkdown = imageLocalPaths.map(p => `![image](file://${p})`).join('\n');
  finalContent = finalContent
    ? `${finalContent}\n\n${imageMarkdown}`
    : imageMarkdown;
}
```

#### 2.3.2 文件 → 文本内容嵌入

解析后的文件内容直接作为文本附加到消息体中：

```typescript
// file: dingtalk-openclaw-connector/src/core/message-handler.ts (约第 1350 行)
if (parseResult.type === 'text' && parseResult.content) {
  fileContentParts.push(
    `📄 **${fileType}**: ${fileName}\n` +
    `✅ 已解析文件内容（${parseResult.content.length} 字符）\n` +
    `📝 内容预览:\n\`\`\`\n${contentPreview}\n\`\`\`\n\n` +
    `📋 完整内容:\n${parseResult.content}`
  );
} else {
  fileContentParts.push(
    `📎 **${fileType}**: ${fileName}\n` +
    `💾 已保存到本地: ${localPath}\n` +
    `🔗 [点击下载](${downloadUrl})`
  );
}

userContent = userContent ? `${userContent}\n\n${fileText}` : fileText;
```

#### 2.3.3 最终分发

构建好的 `finalContent`（包含文本 + 图片 Markdown + 文件内容）通过 OpenClaw SDK 的 `dispatchReplyFromConfig` 发送给 LLM：

```typescript
const ctxPayload = core.channel.reply.finalizeInboundContext({
  Body: body,        // 包含图片和文件内容的完整消息
  BodyForAgent: finalContent,
  From: senderId,
  To: toField,
  // ...
});

await core.channel.reply.dispatchReplyFromConfig({
  ctx: ctxPayload,
  cfg,
  dispatcher,
  replyOptions,
});
```

---

## 3. 核心差异对比表

| 维度 | 微信 (iLink Bot) | 钉钉 (Stream SDK + Connector) |
|------|------------------|-------------------------------|
| **文件引用方式** | `CDNMedia.encrypt_query_param` + `aes_key` | `downloadCode`（统一机制） |
| **传输加密** | AES-128-ECB 加密后上传 CDN，下载后解密 | 通过 `downloadCode` 换取临时 `downloadUrl`，HTTPS 直链下载 |
| **入站下载方式** | CDN 下载密文 → AES-128-ECB 解密 → 保存本地 | `downloadCode` → API 换取 downloadUrl → 下载到本地 |
| **入站文件路径** | `MediaPath` 字段（`WeixinMsgContext`） | `file://` 格式嵌入到 `Body` 文本中 |
| **发送给 LLM 的方式** | `ctx.MediaPath`（本地路径）+ `ctx.MediaType` | `![image](file://path)` Markdown 语法 / 文件内容文本嵌入 |
| **文件解析** | 不解析，仅传递路径 | 自动解析 docx/pdf/txt 等文件内容为文本 |
| **出站上传方式** | 读取文件 → MD5 → AES 加密 → 上传 CDN | 读取文件 → multipart/form-data → 上传 OAPI |
| **出站消息体** | 携带 `encrypt_query_param` + `aes_key` 的 MessageItem | 携带 `mediaId` 的消息体 |
| **CDN URL 时效性** | CDN URL 非临时（加密参数长期有效） | `downloadUrl` 为临时 URL（有时效性） |
| **Rust SDK 层** | 无（项目基于 TypeScript） | 有完整的 Rust SDK，定义了 `DingtalkResource`/`DingTalkMedia` trait |
| **视频入站处理** | CDN 下载 → AES 解密 → 保存为 `video/mp4` → 本地路径传 LLM | `downloadCode` → 换取 URL → 下载保存 → 本地路径传 LLM |
| **视频元数据提取** | 不提取（无 ffprobe 依赖） | 支持 ffprobe 提取时长、分辨率，可生成封面截图 |
| **语音/音频入站** | CDN 下载 → AES 解密 → **SILK→WAV 转码** → 传 LLM | `downloadCode` → 下载保存 → 同时传递语音 `recognition` 文本 |
| **语音识别文本** | 依赖服务端 `voice_item.text` 字段（有则直接用文字，不下载）| 依赖服务端 `recognition` 字段，优先作为消息正文 |
| **音频格式转换** | SILK → WAV（使用 `silk-wasm` 解码，封装 WAV 头） | 不转换，直接保存原始格式（Rust SDK 统一保存为 `.mp3`）|
| **大文件分块上传** | 不支持（微信 CDN 上传无分块机制） | 支持视频/文件 >20MB 自动分块上传（3 步流程） |
| **出站媒体标记** | LLM 回复通过 MIME 类型自动路由（`video/*` → 视频发送） | LLM 回复使用特殊标记 `[DINGTALK_VIDEO]`/`[DINGTALK_AUDIO]` 包裹 |

---

## 4. 总结

### 微信核心链路

```
入站：
  用户消息 → getupdates API → item_list → downloadMediaFromItem()
    → fetchCdnBytes() + decryptAesEcb() → saveMedia()
    → weixinMessageToMsgContext(MediaPath) → dispatchReplyFromConfig() → LLM

出站：
  LLM 回复 → sendWeixinMediaFile() → uploadMediaToCdn()
    → AES 加密 + CDN 上传 → sendImageMessageWeixin() / sendFileMessageWeixin()
    → sendMessage API → 用户
```

### 钉钉核心链路

```
入站：
  用户消息 → CallbackMessage → extractMessageContent(downloadCode)
    → downloadMediaByCode() / downloadFileToLocal()
      → downloadCode 换取 downloadUrl → 下载文件到本地
    → 图片: ![image](file://path) 嵌入文本
    → 文件: parseFileContent() → 文本嵌入
    → 视频/音频: 下载保存 + 路径/识别文本注入
    → dispatchReplyFromConfig() → LLM

出站：
  LLM 回复 → processLocalImages() / processVideoMarkers() / processAudioMarkers() / uploadAndReplaceFileMarkers()
    → uploadMediaToDingTalk() → OAPI media/upload → 替换为 mediaId/downloadUrl
    → sendProactive() / sessionWebhook → 用户
```

### 关键设计差异

1. **加密方式**：微信使用 AES-128-ECB + CDN 私有协议，钉钉使用 HTTPS + 临时 URL
2. **文件传递**：微信通过 `MediaPath` 字段传递本地路径，钉钉通过文本嵌入 `file://` 路径
3. **文件解析**：钉钉 Connector 对文件有更强的解析能力（Word/PDF 提取文本），微信 SDK 仅下载不解析
4. **SDK 架构**：钉钉有完整的 Rust Stream SDK 定义接口和数据结构，微信 SDK 基于 TypeScript 直接实现
5. **语音处理**：微信使用 SILK 编码，需 `silk-wasm` 转码为 WAV 才能被 LLM 使用；钉钉语音通过服务端语音识别直接输出文本，不依赖本地转码
6. **视频处理**：微信仅做下载解密，不提取元数据；钉钉可提取时长/分辨率并生成封面截图（依赖 ffmpeg）
7. **大文件上传**：钉钉支持 >20MB 视频/文件的分块上传，微信 CDN 无此机制

---

## 5. 项目（novaclaw/backend）自身实现分析

### 5.1 项目架构总览

项目采用分层架构，核心路径如下：

```
IM 平台（微信/钉钉）
  ↓ 平台原生消息
适配层（weixin/adapter.rs / dingtalk/adapter.rs）
  ↓ IncomingMessage（统一格式）
IM 网关（im/gateway.rs）
  ↓ 格式化为 LLM 消息
Agent Runtime（agent/runtime.rs）
  ↓ LLM 回复
IM 网关 → 适配层 → 平台 API → 用户
```

**关键文件路径：**

| 层级 | 文件 | 说明 |
|------|------|------|
| 抽象层 | `backend/src/im/types.rs` | `IncomingMessage`、`MessageTarget`、`PlatformCapabilities` 定义 |
| 抽象层 | `backend/src/im/adapter.rs` | `IMAdapter` trait 定义 |
| 抽象层 | `backend/src/im/gateway.rs` | `IMGateway` 消息路由与 Agent 对接 |
| 抽象层 | `backend/src/im/session.rs` | 会话管理和消息格式化 |
| 微信适配 | `backend/src/weixin/adapter.rs` | 微信 `IMAdapter` 实现 |
| 微信客户端 | `backend/src/weixin/client.rs` | 微信 iLink API 客户端 |
| 微信 CDN | `backend/src/weixin/cdn.rs` | AES-128-ECB 加密解密、CDN 上传/下载 |
| 微信上传 | `backend/src/weixin/upload.rs` | 媒体文件上传到微信 CDN 管线 |
| 钉钉适配 | `backend/src/dingtalk/adapter.rs` | 钉钉 `IMAdapter` 实现 |
| 钉钉消息 | `backend/src/dingtalk/message.rs` | 钉钉 REST API 消息发送 |
| 钉钉回调 | `backend/src/dingtalk/handler.rs` | 钉钉回调处理器注册表 |
| 入站处理 | `backend/src/im/handler.rs` | `IMGatewayCallbackHandler` 钉钉消息 → `IncomingMessage` |

### 5.2 统一消息类型定义

**文件：** `backend/src/im/types.rs`

```rust
/// 标准化入站消息
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub id: String,
    pub account_id: String,
    pub account_name: Option<String>,
    pub platform: PlatformType,
    pub conversation_id: String,
    pub sender_id: Option<String>,
    pub sender_staff_id: Option<String>,  // 钉钉真实用户 ID
    pub sender_name: Option<String>,
    pub text: String,                     // 文本内容
    pub media_urls: Vec<String>,          // ★ 媒体资源 URL 列表（Base64 data URL）
    pub raw: serde_json::Value,
    pub session_webhook: Option<String>,  // 钉钉独有
    pub conversation_type: ConversationType,
    pub conversation_title: Option<String>,
    pub timestamp: i64,
}

/// 平台能力声明
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    pub supports_markdown: bool,
    pub supports_images: bool,
    pub supports_files: bool,
    pub max_message_length: usize,
}
```

**设计亮点：**
- `media_urls` 统一为 `Vec<String>`，存储 Base64 data URL（而非本地路径），无需额外文件管理
- `PlatformCapabilities` 声明平台能力，用于发送时降级路由
- `ConversationType::from_dingtalk()` 将钉钉字符串映射为统一枚举

### 5.3 IMAdapter 契约

**文件：** `backend/src/im/adapter.rs`

```rust
#[async_trait]
pub trait IMAdapter: Send + Sync {
    fn platform_type(&self) -> PlatformType;
    fn is_connected(&self) -> bool;
    fn capabilities(&self) -> PlatformCapabilities;

    async fn send_text(&self, target: &MessageTarget, text: &str) -> Result<SendResult, AppError>;
    async fn send_markdown(&self, target: &MessageTarget, title: &str, text: &str) -> Result<SendResult, AppError>;
    async fn reply(&self, original: &IncomingMessage, text: &str) -> Result<SendResult, AppError>;

    // 图片/文件/视频/音频发送（带默认降级实现）
    async fn send_image(&self, target: &MessageTarget, url: &str, caption: Option<&str>) -> Result<SendResult, AppError> {
        Err(AppError::External("该平台不支持发送图片".to_string()))  // 默认返回错误
    }
    async fn send_file(&self, target: &MessageTarget, url: &str, file_name: &str) -> Result<SendResult, AppError> {
        Err(AppError::External("该平台不支持发送文件".to_string()))
    }
    async fn send_video(&self, target: &MessageTarget, url: &str, caption: Option<&str>) -> Result<SendResult, AppError> {
        Err(AppError::External("该平台不支持发送视频".to_string()))
    }
    async fn send_audio(&self, target: &MessageTarget, url: &str) -> Result<SendResult, AppError> {
        Err(AppError::External("该平台不支持发送音频".to_string()))
    }

    async fn start_stream_reply(&self, original: &IncomingMessage) -> Result<mpsc::UnboundedSender<String>, AppError>;
    async fn finish_stream_reply(&self, original: &IncomingMessage) -> Result<(), AppError>;
}
```

**设计说明：**
- `send_image` / `send_file` / `send_video` / `send_audio` 都有默认实现（返回错误），各平台可选择性地覆盖实现
- 每个方法接受 `url` 参数（远程资源地址），适配器内部负责下载 → 上传 → 发送的三步流程

### 5.4 微信适配层实现

#### 5.4.1 入站消息处理

**文件：** `backend/src/weixin/adapter.rs`（函数 `convert_incoming_async`）

```rust
/// 将微信消息转为统一 IncomingMessage（异步版本，支持 CDN 图片下载）
async fn convert_incoming_async(
    msg: &WeixinMessage,
    account_id: &str,
    account_name: &Option<String>,
    cdn_base_url: &str,
) -> Option<IncomingMessage> {
    let items = msg.item_list.as_deref().unwrap_or(&[]);

    // 提取文本内容
    let text: String = items.iter()
        .filter(|item| item.item_type == msg_item_type::TEXT)
        .filter_map(|item| item.text_item.as_ref())
        .filter_map(|t| Some(t.text.clone()))
        .collect::<Vec<_>>().join(" ");

    // 处理媒体项：下载 CDN 图片 → AES 解密 → Base64 data URL
    let mut media_urls: Vec<String> = Vec::new();

    for item in items {
        match item.item_type {
            msg_item_type::IMAGE => {
                // ★ 已实现：下载 CDN 图片并解密为 Base64 data URL
                if let Some(img) = &item.image_item {
                    if let Some(media) = &img.media {
                        // download_weixin_cdn_image_to_base64() 完整实现
                        // CDN 下载 → AES-128-ECB 解密 → 格式探测 → Base64 data URL
                        match download_weixin_cdn_image_to_base64(
                            encrypt_param, aes_key, full_url, cdn_base_url,
                        ).await {
                            Ok(data_url) => media_urls.push(data_url),
                            Err(e) => tracing::warn!("..."),
                        }
                    }
                }
            }
            msg_item_type::FILE => {
                // ✅ 已实现：下载 CDN 文件 → AES 解密 → 文本文件提取内容嵌入消息
                tracing::info!("[微信] 检测到文件消息项");
                if let Some(file) = &item.file_item {
                    let file_name = file.file_name.as_deref().unwrap_or("未知文件");
                    if let Some(media) = &file.media {
                        let encrypt_param = media.encrypt_query_param.as_deref().unwrap_or("");
                        let aes_key = media.aes_key.as_deref().unwrap_or("");
                        if !encrypt_param.is_empty() && !aes_key.is_empty() {
                            match download_weixin_cdn_media_bytes(
                                encrypt_param, aes_key, media.full_url.as_deref(), cdn_base_url,
                            ).await {
                                Ok(bytes) => {
                                    if is_text_file(file_name) {
                                        let content = String::from_utf8_lossy(&bytes);
                                        let preview = if content.len() > 2000 {
                                            format!("{}...\n[内容过长，已截断]", &content[..2000])
                                        } else { content.to_string() };
                                        text = format!("[用户发送了一个文本文件: {}]\n---\n{}", file_name, preview);
                                    } else {
                                        text = format!("[用户发送了一个文件: {} ({} 字节)]", file_name, bytes.len());
                                    }
                                }
                                Err(e) => text = format!("[用户发送了一个文件: {} (下载失败: {})]", file_name, e),
                            }
                        }
                    }
                }
            }
            msg_item_type::VIDEO => {
                // ✅ 已实现：下载 CDN 视频 → 解密 → 描述信息
                tracing::info!("[微信] 检测到视频消息项");
                if let Some(video) = &item.video_item {
                    if let Some(media) = &video.media {
                        let encrypt_param = media.encrypt_query_param.as_deref().unwrap_or("");
                        let aes_key = media.aes_key.as_deref().unwrap_or("");
                        if !encrypt_param.is_empty() && !aes_key.is_empty() {
                            match download_weixin_cdn_media_bytes(
                                encrypt_param, aes_key, media.full_url.as_deref(), cdn_base_url,
                            ).await {
                                Ok(bytes) => {
                                    let size_mb = bytes.len() as f64 / 1024.0 / 1024.0;
                                    text = format!("[用户发送了一个视频 ({:.1} MB)]", size_mb);
                                }
                                Err(e) => text = format!("[视频下载失败: {}]", e),
                            }
                        } else {
                            text = "[用户发送了一个视频]".to_string();
                        }
                    }
                }
            }
            msg_item_type::VOICE => {
                // ✅ 已实现：优先使用服务端语音转文字结果，无结果则下载语音文件
                tracing::info!("[微信] 检测到语音消息项");
                if let Some(voice) = &item.voice_item {
                    if let Some(transcript) = &voice.text {
                        let transcript = transcript.trim();
                        if !transcript.is_empty() {
                            text = format!("[用户发送了一条语音, 转文字: {}]", transcript);
                        }
                    } else if let Some(media) = &voice.media {
                        let encrypt_param = media.encrypt_query_param.as_deref().unwrap_or("");
                        let aes_key = media.aes_key.as_deref().unwrap_or("");
                        if !encrypt_param.is_empty() && !aes_key.is_empty() {
                            match download_weixin_cdn_media_bytes(
                                encrypt_param, aes_key, media.full_url.as_deref(), cdn_base_url,
                            ).await {
                                Ok(bytes) => {
                                    let size_kb = bytes.len() as f64 / 1024.0;
                                    text = format!("[用户发送了一条语音 ({:.1} KB, SILK 格式)]", size_kb);
                                }
                                Err(e) => text = format!("[语音下载失败: {}]", e),
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // 纯图片消息（无文字）使用占位文本
    let final_text = if text.is_empty() {
        if !media_urls.is_empty() {
            "[用户发送了一张图片]".to_string()
        } else {
            return None;  // 无文字也无媒体则跳过
        }
    } else { text };

    Some(IncomingMessage {
        id: format!("wx_{}", msg.message_id.unwrap_or(0)),
        account_id: account_id.to_string(),
        platform: PlatformType::Custom("weixin".to_string()),
        conversation_id: conv_id.to_string(),
        sender_id: Some(from.to_string()),
        text: final_text,
        media_urls,
        // 微信 iLink SDK 不支持群聊，统一私聊
        conversation_type: ConversationType::Private,
        // ...
    })
}
```

**通用媒体下载函数链（文件/视频/语音共用）：**

```
convert_incoming_async()
  ├→ download_weixin_cdn_image_to_base64()      ← 图片专用（输出 Base64 data URL）
  │    ├→ 解析 AES key（base64 → 16 字节 → 兼容 hex 编码）
  │    ├→ 构建 CDN 下载 URL（cdn.rs: build_cdn_download_url）
  │    ├→ reqwest::get() 下载加密密文
  │    ├→ cdn.rs: decrypt_aes_ecb() AES-128-ECB 解密
  │    ├→ detect_image_format() 探测图片格式（jpg/png/gif/webp）
  │    └→ base64::encode → data:image/xxx;base64,... 包装
  │
  ├→ download_weixin_cdn_media_bytes()           ← 文件/视频/语音通用
  │    ├→ 解析 AES key（兼容 base64 和 hex 编码）
  │    ├→ download_raw_and_decrypt()
  │    │    ├→ 构建 CDN 下载 URL
  │    │    ├→ reqwest::get() 下载加密密文
  │    │    └→ cdn.rs: decrypt_aes_ecb() AES-128-ECB 解密
  │    └→ 返回原始字节 Vec<u8>
  │
  ├→ is_text_file(file_name)                     ← 文件辅助检测
  │    └→ 根据扩展名判断是否为文本文件（txt/md/json/csv/log/代码文件等）
  │
  └→ 处理逻辑：
       ├→ FILE: 下载 → is_text_file判断 → 文本：提取内容嵌入消息；二进制：描述大小
       ├→ VIDEO: 下载 → 计算大小 → 描述 "[用户发送了一个视频 (X.X MB)]"
       └→ VOICE: 优先 voice.text 转文字 → 无结果则下载描述大小
```

**文件：** `backend/src/weixin/cdn.rs`

```rust
/// AES-128-ECB 解密（移除 PKCS7 padding）
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8]) -> Result<Vec<u8>, AppError> {
    use aes::cipher::{BlockDecrypt, KeyInit};
    use aes::cipher::generic_array::GenericArray;
    // key 长度校验 → 密文长度校验 → ECB 逐块解密 → PKCS7 去填充
}

/// 构建 CDN 下载 URL
pub fn build_cdn_download_url(cdn_base_url: &str, encrypted_query_param: &str) -> String {
    format!("{}/download?encrypted_query_param={}",
        cdn_base_url.trim_end_matches('/'), urlencoding::encode(encrypted_query_param))
}
```

**消息结构体定义（文件：** `backend/src/weixin/client.rs` **）：**

```rust
/// 微信消息项类型枚举值
pub mod msg_item_type {
    pub const TEXT: i32 = 1;
    pub const IMAGE: i32 = 2;
    pub const FILE: i32 = 3;
    pub const VIDEO: i32 = 4;
    pub const VOICE: i32 = 5;
}

/// 消息项（支持多模态）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    #[serde(rename = "type")]
    pub item_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_item: Option<ImageItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_item: Option<FileItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_item: Option<VideoItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_item: Option<VoiceItem>,  // ← 新增：语音消息支持
}

/// 语音消息项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CDNMedia>,
    /// 服务端语音转文字结果（若有则无需下载语音）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}
```

#### 5.4.2 出站消息处理

**文件：** `backend/src/weixin/adapter.rs`（`IMAdapter for WeixinAdapter`）

```rust
#[async_trait]
impl IMAdapter for WeixinAdapter {
    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            supports_markdown: false,     // 微信不支持 Markdown
            supports_images: true,        // ✅ 支持图片
            supports_files: true,         // ✅ 支持文件
            max_message_length: 4000,
        }
    }

    /// 发送图片（✅ 已实现）
    async fn send_image(&self, target: &MessageTarget, url: &str, caption: Option<&str>) -> Result<SendResult, AppError> {
        // 1. 下载远程图片到临时文件
        let temp_path = download_remote_image(url).await?;
        // 2. 上传到微信 CDN（upload.rs: upload_file_to_weixin）
        let uploaded = upload::upload_file_to_weixin(&self.client, &temp_path, &target.conversation_id).await?;
        // 3. 发送图片消息（client.rs: send_image_message）
        self.client.send_image_message(
            &target.conversation_id, caption_text,
            &uploaded.download_param, &uploaded.aeskey_base64,
            uploaded.file_size_ciphertext, None,
        ).await?;
        // 4. 清理临时文件
        Ok(SendResult::ok())
    }

    /// 发送文件（✅ 已实现）
    async fn send_file(&self, target: &MessageTarget, url: &str, file_name: &str) -> Result<SendResult, AppError> {
        let temp_path = download_remote_image(url).await?;
        let uploaded = upload::upload_file_attachment_to_weixin(&self.client, &temp_path, &target.conversation_id).await?;
        self.client.send_file_message(
            &target.conversation_id, "", file_name,
            &uploaded.download_param, &uploaded.aeskey_base64,
            uploaded.file_size, None,
        ).await?;
        Ok(SendResult::ok())
    }

    /// 发送视频（✅ 已实现）
    async fn send_video(&self, target: &MessageTarget, url: &str, caption: Option<&str>) -> Result<SendResult, AppError> {
        // 1. 下载远程视频到临时文件
        let temp_path = download_remote_image(url).await?;
        // 2. 上传到微信 CDN（使用 VIDEO 媒体类型）
        let uploaded = upload::upload_video_to_weixin(&self.client, &temp_path, &target.conversation_id).await?;
        // 3. 发送视频消息
        let caption_text = caption.unwrap_or("");
        self.client.send_video_message(
            &target.conversation_id, caption_text,
            &uploaded.download_param, &uploaded.aeskey_base64,
            uploaded.file_size_ciphertext, None,
        ).await?;
        Ok(SendResult::ok())
    }
}
```

**出站上传管线（文件：** `backend/src/weixin/upload.rs` **）：**

```rust
/// 通用媒体上传管线
async fn upload_media_to_cdn(client: &WeixinClient, file_path: &str, to_user_id: &str, media_type: i32) -> Result<UploadedFileInfo, AppError> {
    // 1. 读取本地文件
    let plaintext = tokio::fs::read(path).await?;
    // 2. MD5 哈希
    let rawfilemd5 = format!("{:x}", Md5::digest(&plaintext));
    // 3. 生成 AES 密钥（16 字节随机 hex）
    let aeskey_bytes = random_hex_16()[..16].as_bytes().to_vec();
    // 4. getUploadUrl API 获取上传地址
    let upload_url_resp = client.get_upload_url(&upload_req).await?;
    // 5. AES-128-ECB 加密（cdn.rs: encrypt_aes_ecb）
    let ciphertext = cdn::encrypt_aes_ecb(&plaintext, &aeskey_bytes)?;
    // 6. 上传到 CDN（cdn.rs: upload_buffer_to_cdn）
    let cdn_result = cdn::upload_buffer_to_cdn(&ciphertext, ...).await?;

    Ok(UploadedFileInfo {
        download_param: cdn_result.download_param,
        aeskey_hex, aeskey_base64,
        file_size, file_size_ciphertext,
    })
}

/// 类型路由
pub async fn upload_file_to_weixin(client, file_path, to_user_id)       // media_type=1 (IMAGE)
pub async fn upload_video_to_weixin(client, file_path, to_user_id)      // media_type=2 (VIDEO)
pub async fn upload_file_attachment_to_weixin(client, file_path, to_user_id) // media_type=3 (FILE)
```

**文件：** `backend/src/weixin/client.rs`（消息发送 API）

```rust
// ★ 微信 API 客户端已实现以下媒体发送方法：

pub async fn send_text(&self, to_user_id: &str, text: &str, context_token: Option<&str>)
pub async fn send_image_message(&self, to_user_id, text, download_param, aes_key_base64, file_size_ciphertext, context_token)
pub async fn send_video_message(&self, to_user_id, text, download_param, aes_key_base64, file_size_ciphertext, context_token)
pub async fn send_file_message(&self, to_user_id, text, file_name, download_param, aes_key_base64, file_size, context_token)
```

**注意**：`client.rs` 中 `send_video_message` 已实现，但 `adapter.rs` 中 `IMAdapter` 的实现**未覆盖视频发送**（IMAdapter trait 没有 `send_video` 方法，当前通过 `send_file` 降级处理）。

### 5.5 钉钉适配层实现

#### 5.5.1 入站消息处理

**文件：** `backend/src/im/handler.rs`（`IMGatewayCallbackHandler`）

```rust
#[async_trait]
impl CallbackHandler for IMGatewayCallbackHandler {
    async fn on_callback_message(&self, msg: CallbackMessageData, _session_webhook: Option<String>) {
        let mut media_urls: Vec<String> = Vec::new();
        let mut text = msg.text.as_ref().map(|t| t.content.clone()).unwrap_or_default();

        match msg.msgtype.as_str() {
            "picture" => {
                text = "[用户发送了一张图片]".to_string();
                Self::download_picture(&self.client, &msg.content, &mut media_urls).await;
            }
            "richText" => {
                // 遍历 richText 数组，提取文本片段和图片 downloadCode
                // 逐个下载图片并转为 Base64 data URL
                for item in rich_text.iter() {
                    match item_type {
                        "text" => text_buf.push_str(t),
                        "picture" => {
                            // 逐个下载图片
                            let code = item.get("downloadCode").and_then(|v| v.as_str());
                            if let Some(code) = code {
                                // 调用 client.download_media_to_base64() 下载
                            }
                        }
                    }
                }
            }
            "file" => {
                // ✅ 已实现：下载文件 → 检测文本类型 → 提取内容嵌入消息
                tracing::info!("[钉钉] 检测到文件消息项");
                text = Self::download_file_and_extract_text(&self.client, &msg.content).await;
                if text.is_empty() {
                    text = "[用户发送了一个文件]".to_string();
                }
            }
            "video" => {
                // ✅ 已实现：提取视频信息（时长、格式）
                tracing::info!("[钉钉] 检测到视频消息项");
                let duration = msg.content.as_ref()
                    .and_then(|c| c.get("duration"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知");
                let video_type = msg.content.as_ref()
                    .and_then(|c| c.get("videoType"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知格式");
                text = format!("[用户发送了一个视频: 时长={}秒, 格式={}]", duration, video_type);
            }
            "voice" => {
                // ✅ 已实现：优先使用服务端语音识别文本
                tracing::info!("[钉钉] 检测到语音消息项");
                let recognition = msg.content.as_ref()
                    .and_then(|c| c.get("recognition"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !recognition.is_empty() {
                    text = format!("[用户发送了一条语音, 转文字: {}]", recognition);
                } else {
                    text = "[用户发送了一条语音]".to_string();
                }
            }
        }

        let incoming_msg = IncomingMessage {
            text, media_urls, // media_urls 包含 Base64 data URL
            // ...
        };
        self.incoming_tx.send(incoming_msg).ok();
    }
}
```

**入站文件下载与文本提取辅助函数（文件：** `backend/src/im/handler.rs` **）：**

```rust
/// 检测文件扩展名是否为文本类型
fn is_text_file_ext(file_name: &str) -> bool {
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(
        ext.as_str(),
        "txt" | "md" | "json" | "csv" | "log" | "xml" | "yaml" | "yml"
            | "toml" | "ini" | "cfg" | "conf" | "sh" | "bat" | "ps1"
            | "py" | "js" | "ts" | "rs" | "go" | "java" | "c" | "cpp"
            | "h" | "hpp" | "html" | "css" | "scss" | "less" | "sql"
    )
}

/// 下载文件并提取文本内容（若为文本文件）
async fn download_file_and_extract_text(
    client: &Option<Arc<DingTalkClient>>,
    content: &Option<serde_json::Value>,
) -> String {
    // 1. 从 content 提取 downloadCode 和 fileName
    let download_code = content.get("downloadCode").and_then(|v| v.as_str());
    let file_name = content.get("fileName").and_then(|v| v.as_str()).unwrap_or("未知文件");
    let Some(code) = download_code else {
        return format!("[用户发送了一个文件: {}]", file_name);
    };

    // 2. 通过 downloadCode 换取下载 URL
    let download_url = client.download_file(code).await?;
    // 3. 下载文件内容
    let bytes = reqwest::get(&download_url).await?.bytes().await?;

    // 4. 文本文件 → 提取内容（截断超过 3000 字符）
    if is_text_file_ext(file_name) {
        let content_str = String::from_utf8_lossy(&bytes);
        let preview = if content_str.len() > 3000 {
            format!("{}...\n[内容过长已截断, 共 {} 字符]", &content_str[..3000], content_str.len())
        } else { content_str.to_string() };
        format!("[用户发送了一个文本文件: {}]\n---\n{}", file_name, preview)
    } else {
        format!("[用户发送了一个文件: {} ({} 字节)]", file_name, bytes.len())
    }
}
```

**入站图片下载链（文件：** `backend/src/dingtalk/message.rs` **）：**

```rust
/// 下载消息中的媒体文件并转为 Base64 data URL
pub async fn download_media_to_base64(&self, download_code: &str, mime_type: &str) -> Result<String, AppError> {
    // 1. downloadFile API → 换取临时 download URL
    let download_url = self.download_file(download_code).await?;
    // 2. 下载二进制内容
    let bytes = self.http_client.get(&download_url).send().await?.bytes().await?;
    // 3. Base64 编码 → data URL
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime_type, b64))
}

/// downloadFile API 调用
pub async fn download_file(&self, download_code: &str) -> Result<String, AppError> {
    let token = self.token_manager.get_token().await?;
    // POST https://api.dingtalk.com/v1.0/robot/messageFiles/download
    // { robotCode, downloadCode } → 返回 { downloadUrl }
}
```

#### 5.5.2 出站消息处理

**文件：** `backend/src/dingtalk/adapter.rs`（`IMAdapter for DingTalkAdapter`）

```rust
#[async_trait]
impl IMAdapter for DingTalkAdapter {
    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            supports_markdown: true,    // ✅ 支持 Markdown
            supports_images: true,      // ✅ 支持图片
            supports_files: true,       // ✅ 支持文件发送
            max_message_length: 20000,
        }
    }

    /// 发送图片（✅ 已实现）
    async fn send_image(&self, target: &MessageTarget, url: &str, _caption: Option<&str>) -> Result<SendResult, AppError> {
        match target.conversation_type {
            Private => self.client.send_private_image(vec![target.conversation_id.clone()], url).await?,
            Group   => self.client.send_group_image(&target.conversation_id, url).await?,
        }
        Ok(SendResult::ok())
    }

    /// 发送文件（✅ 已实现）
    async fn send_file(&self, target: &MessageTarget, url: &str, file_name: &str) -> Result<SendResult, AppError> {
        match target.conversation_type {
            Private => self.client.send_private_file(vec![target.conversation_id.clone()], url, file_name).await?,
            Group   => self.client.send_group_file(&target.conversation_id, url, file_name).await?,
        }
        Ok(SendResult::ok())
    }

    /// 发送视频（✅ 已实现）
    async fn send_video(&self, target: &MessageTarget, url: &str, _caption: Option<&str>) -> Result<SendResult, AppError> {
        let file_name = "video.mp4";
        match target.conversation_type {
            Private => self.client.send_private_video(vec![target.conversation_id.clone()], url, file_name).await?,
            Group   => self.client.send_group_video(&target.conversation_id, url, file_name).await?,
        }
        Ok(SendResult::ok())
    }

    /// 发送音频（✅ 已实现）
    async fn send_audio(&self, target: &MessageTarget, url: &str) -> Result<SendResult, AppError> {
        match target.conversation_type {
            Private => self.client.send_private_audio(vec![target.conversation_id.clone()], url).await?,
            Group   => self.client.send_group_audio(&target.conversation_id, url).await?,
        }
        Ok(SendResult::ok())
    }
}
```

**钉钉图片发送 API（文件：** `backend/src/dingtalk/message.rs` **）：**

```rust
/// 发送私聊图片
pub async fn send_private_image(&self, user_ids: Vec<String>, photo_url: &str) -> Result<(), AppError> {
    let request = PrivateMessageRequest {
        msg_param: serde_json::json!({"photoURL": photo_url}).to_string(),
        msg_key: MSG_KEY_IMAGE.to_string(),
    };
    // POST https://api.dingtalk.com/v1.0/robot/oToMessages/batchSend
}

/// 发送群聊图片
pub async fn send_group_image(&self, open_conversation_id: &str, photo_url: &str) -> Result<(), AppError> {
    let request = GroupMessageRequest {
        msg_param: serde_json::json!({"photoURL": photo_url}).to_string(),
        msg_key: MSG_KEY_IMAGE.to_string(),
    };
    // POST https://api.dingtalk.com/v1.0/robot/groupMessages/send
}

/// 上传媒体文件到钉钉 OAPI（可用于发送文件→先上传获取 mediaId）
pub async fn upload_media(&self, media_type: &str, file_data: Vec<u8>, file_name: &str) -> Result<MediaUploadResponse, AppError> {
    // POST https://oapi.dingtalk.com/media/upload?access_token=<token>&type=<media_type>
    // multipart/form-data 上传
}
```

**流式回复（AI Card）：**

钉钉适配器通过 AI Card 实现了流式回复（`start_stream_reply`），这是钉钉独有的能力：

```rust
async fn start_stream_reply(&self, original: &IncomingMessage) -> Result<mpsc::UnboundedSender<String>, AppError> {
    // 1. 创建 AI Card（私聊用 sender_staff_id，群聊用 conversation_id）
    let card = self.client.card_create(user_id, conversation_id).await?;
    // 2. 后台任务：逐块流式更新卡片（800ms 节流）
    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            // card_set_inputing → card_stream_update → card_set_finished
        }
    });
    Ok(tx)
}
```

### 5.6 媒体消息到 LLM 的传递路径

**文件：** `backend/src/im/gateway.rs`（`process_single_message`）

```
IncomingMessage.media_urls (Base64 data URL list)
  │
  ▼
runtime.run_turn(&user_text, step_tx, None, &msg.media_urls)
  │                              └──→ images: &[String]
  ▼
self.session.push_user_with_images(&processed_input, images)
  │
  ▼
AgentMessage { role: "user", content, images: Some(image_urls) }
  │
  ▼
call_llm_with_tools() 中构建 ChatMessage
  │
  ▼
LLM API 调用（多模态模型接收 Base64 data URL）
```

关键代码（`gateway.rs`）：

```rust
// 日志记录 media_urls
tracing::info!(
    "[Gateway] 处理消息: text_prefix={}, session_id={}, media_urls数量={}, 首条前缀={}",
    msg.text.chars().take(50).collect::<String>(),
    session_id,
    msg.media_urls.len(),
    msg.media_urls.first().map(|u| &u[..u.len().min(60)]).unwrap_or("(无)")
);

// 传入图片给 Agent
let result = runtime.run_turn(&user_text, step_tx, None, &msg.media_urls).await?;
```

### 5.7 实现完整性评估

#### 已实现功能（✅）

| 功能 | 微信 | 钉钉 |
|------|------|------|
| **入站图片** → Base64 data URL → LLM | ✅ `adapter.rs` → `download_weixin_cdn_image_to_base64` | ✅ `im/handler.rs` → `download_media_to_base64` |
| **入站文件** → 文本提取/描述 → LLM | ✅ `download_weixin_cdn_media_bytes` + `is_text_file` 提取文本 | ✅ `download_file_and_extract_text` |
| **入站视频** → 描述信息 → LLM | ✅ CDN 下载 + 解密 + 大小描述 | ✅ 提取 `duration` + `videoType` |
| **入站语音** → 转文字/描述 → LLM | ✅ 优先 `voice_item.text`，降级下载描述 | ✅ 优先 `recognition` 字段 |
| **出站图片** (Agent → 用户) | ✅ `send_image` → 上传CDN → 发送 | ✅ `send_image` → 直接发送photoURL |
| **出站文件** (Agent → 用户) | ✅ `send_file` → 上传CDN → 发送 | ✅ `send_file` → 上传OAPI → 发送 |
| **出站视频** (Agent → 用户) | ✅ `send_video` → 上传CDN → 发送 | ✅ `send_video` → 上传OAPI → 发送 |
| **出站音频** (Agent → 用户) | ❌ 微信iLink不支持音频发送 | ✅ `send_audio` → 上传OAPI → 发送 |
| **出站文本** | ✅ `send_text` | ✅ `send_text` / `send_markdown` |
| **流式回复** | ❌ 不支持（平台限制） | ✅ AI Card 流式回复 |
| **消息持久化** | ✅ 图片存为文件，路径写入 `image_paths` | ✅ 同上 |

#### 对比官方 SDK 的关键差异

| 维度 | 官方 SDK | 本项目 |
|------|----------|--------|
| **媒体传给 LLM 的方式** | 本地文件路径（`MediaPath` / `file://`） | Base64 data URL（嵌入消息体） |
| **图片入站** | 下载解密 → 本地文件 | 下载解密 → Base64 data URL |
| **文件入站** | 下载 → 保存本地路径 → LLM | 下载 → 文本提取/描述 → LLM |
| **视频入站** | 下载 → 保存本地路径 → LLM | 下载 → 大小描述 → LLM |
| **语音入站** | 下载 → SILK→WAV 转码 → 本地文件 | 优先转文字，降级下载描述 |
| **文件内容解析** | 钉钉 Connector 支持（docx/pdf/txt） | 文本文件提取内容（`is_text_file`），暂不解析二进制文件 |
| **大文件分块上传** | 钉钉 SDK 支持 >20MB 分块上传 | 未实现（直接上传，超过 20MB 可能失败） |

### 5.8 出站功能测试建议

根据代码分析，出站功能的状态如下：

1. **微信出站图片**（`send_image`）：**已实现但未测试**
   - 路径：`download_remote_image(url)` → `upload_file_to_weixin()` → `send_image_message()`
   - 测试方法：在对话中让 LLM 生成图片，观察日志中 `[微信] 发送图片` / `[微信] 图片 CDN 上传完成` / `[微信] 图片发送成功`

2. **微信出站文件**（`send_file`）：**已实现但未测试**
   - 路径：`download_remote_image(url)` → `upload_file_attachment_to_weixin()` → `send_file_message()`
   - 测试方法：让 LLM 调用工具生成文件并通过 `im_push` 推送给微信用户

3. **微信出站视频**（`send_video`）：**已实现但未测试**

4. **钉钉出站图片**（`send_image`）：**已实现且已验证可用**
   - 路径：`send_private_image()` / `send_group_image()` → REST API

5. **钉钉出站文件**（`send_file`）：**已实现但未测试**
   - 路径：下载远程文件 → `upload_media("file")` → `send_private_file`/`send_group_file`

6. **钉钉出站视频**（`send_video`）：**已实现但未测试**
   - 路径：下载远程视频 → `upload_media("video")` → `send_private_video`/`send_group_video`

7. **钉钉出站音频**（`send_audio`）：**已实现但未测试**
   - 路径：下载远程音频 → `upload_media("voice")` → `send_private_audio`/`send_group_audio`

### 5.9 代码文件索引

| 文件路径 | 功能 |
|----------|------|
| `backend/src/im/types.rs` | 统一消息类型 `IncomingMessage`、`MessageTarget`、`PlatformCapabilities` |
| `backend/src/im/adapter.rs` | `IMAdapter` trait（消息发送契约，含 `send_video`/`send_audio`） |
| `backend/src/im/gateway.rs` | `IMGateway`（消息路由、会话管理、Agent 对接） |
| `backend/src/im/handler.rs` | 钉钉入站消息回调处理器（图片/文件/视频/语音处理） |
| `backend/src/im/session.rs` | 会话管理、消息格式化（`format_im_message`） |
| `backend/src/weixin/adapter.rs` | 微信适配器（入站消息转换、出站图片/文件/视频发送） |
| `backend/src/weixin/client.rs` | 微信 iLink API 客户端 + `MessageItem` 结构体定义（含 `VoiceItem`） |
| `backend/src/weixin/cdn.rs` | AES-128-ECB 加密解密、CDN URL 构建、CDN 上传 |
| `backend/src/weixin/upload.rs` | 媒体文件上传到微信 CDN 管线（图片/文件/视频三种类型） |
| `backend/src/dingtalk/adapter.rs` | 钉钉适配器（图片/文件/视频/音频发送、流式回复 AI Card） |
| `backend/src/dingtalk/message.rs` | 钉钉 REST API 消息发送（文本/图片/文件/视频/音频上传和发送） |
| `backend/src/dingtalk/frames.rs` | 钉钉消息帧结构（含文件/视频/音频消息 key 和参数结构） |
| `backend/src/dingtalk/handler.rs` | 钉钉回调处理器注册表 |
| `backend/src/tools/builtin/im_push.rs` | `im_push` 工具（LLM 通过工具主动推送消息到 IM 平台） |