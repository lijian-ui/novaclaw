export interface Session {
  id: string
  name: string
  created_at: string
  updated_at: string
  model: string
  metadata?: string
}

export interface Message {
  id: string
  session_id: string
  role: string
  content: string
  created_at: string
  metadata?: string
}

export interface Model {
  id: string
  name: string
  provider: string
  context_window: number
  max_tokens: number
}

export interface Skill {
  id: string
  name: string
  description: string
  version: string
  level: number
  enabled: boolean
  lifecycle?: string
  use_count?: number
  tags?: string[]
}

export interface CronJob {
  id: string
  name: string
  schedule: string
  enabled: boolean
  payload: string
  created_at: string
  updated_at: string
}

export interface Layout {
  id: string
  user_id: string
  name: string
  content: string
  created_at: string
  updated_at: string
}

export interface ChatRequest {
  session_id?: string
  message: string
  model?: string
  stream: boolean
}

export interface ChatResponse {
  session_id: string
  message_id: string
  content: string
  role: string
}

export interface Config {
  server: ServerConfig
  llm: LlmConfig
  security: SecurityConfig
}

export interface ServerConfig {
  port: number
  host: string
}

export interface LlmConfig {
  timeout: number
  max_retries: number
  default_model: string
  providers: ProviderConfig[]
}

export interface ProviderConfig {
  name: string
  api_key: string
  base_url: string
  models: string[]
}

export interface SecurityConfig {
  allowed_origins: string[]
  prompt_injection_protection: boolean
}