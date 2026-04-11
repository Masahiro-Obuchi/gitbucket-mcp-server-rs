# GitBucket MCP Server (Rust) - 実現性検討・仕様・実装計画

## 1. 実現性検討

### 1.1 結論: **実現可能**

以下の3つの観点から、GitBucket MCPサーバーのRust実装は十分に実現可能です。

### 1.2 MCP Rust SDK の成熟度

| 項目 | 詳細 |
|------|------|
| SDK | `rmcp` v1.2.0（公式 Rust MCP SDK） |
| リポジトリ | https://github.com/modelcontextprotocol/rust-sdk |
| プロトコル準拠 | MCP 2025-03-26 以降対応 |
| トランスポート | stdio, Streamable HTTP, SSE |
| マクロ | `#[tool]`, `#[tool_router]`, `#[tool_handler]` によるエルゴノミックなAPI |
| 非同期 | tokio ベース |

rmcpは公式SDKとして活発にメンテナンスされており、プロシージャルマクロによりツール定義が簡潔に記述可能です。

### 1.3 GitBucket REST API の互換性

GitBucket REST API v3 は GitHub API v3 のサブセット互換です。`gitbucket-cli-rs` の参考実装で以下のエンドポイントが動作確認済み:

- **ユーザー**: `GET /user`, `GET /users/:username`
- **リポジトリ**: CRUD + fork + branches
- **Issue**: CRUD + state変更 + コメント
- **Pull Request**: CRUD + merge + コメント

### 1.4 参考実装の活用

`/home/obuchi/gitbucket-cli-rs` に以下のパターンが実装済みで参考にできます:
- APIクライアント設計（認証、Base URL正規化、レスポンス処理）
- データモデル（User, Repository, Issue, PullRequest, Comment）
- エラーハンドリングパターン
- テスト戦略（単体テスト + モックHTTP + E2Eテスト）

---

## 2. 技術選定

### 2.1 依存クレート

```toml
[dependencies]
rmcp = { version = "1", features = ["server", "transport-io"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls-native-roots"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
url = "2"
toml = "0.8"
dirs = "6"

[dev-dependencies]
wiremock = "0.6"
tokio-test = "0.4"
assert-json-diff = "2"
tempfile = "3"
serial_test = "3.4.0"
```

### 2.2 アーキテクチャ

```
gitbucket-mcp-server-rs/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs                    # エントリポイント（stdio transport起動）
│   ├── lib.rs                     # ライブラリルート
│   ├── server.rs                  # MCPサーバー定義（ServerHandler実装）
│   ├── config.rs                  # 設定（TOMLファイル + 環境変数）
│   ├── error.rs                   # エラー型定義
│   ├── api/
│   │   ├── mod.rs                 # APIモジュールエクスポート
│   │   ├── client.rs              # GitBucket APIクライアント
│   │   ├── repository.rs          # リポジトリAPI
│   │   ├── issue.rs               # Issue API
│   │   ├── pull_request.rs        # PR API
│   │   └── user.rs                # ユーザーAPI
│   ├── models/
│   │   ├── mod.rs                 # モデルエクスポート
│   │   ├── user.rs                # User構造体
│   │   ├── repository.rs          # Repository構造体
│   │   ├── issue.rs               # Issue, Label構造体
│   │   ├── pull_request.rs        # PullRequest構造体
│   │   └── comment.rs             # Comment構造体
│   └── tools/
│       ├── mod.rs                 # ツールモジュールエクスポート
│       ├── repository.rs          # リポジトリ系MCPツール
│       ├── issue.rs               # Issue系MCPツール
│       ├── pull_request.rs        # PR系MCPツール
│       └── user.rs                # ユーザー系MCPツール
└── tests/
    ├── common/
    │   └── mod.rs                 # テストヘルパー（モックサーバー等）
    ├── api_client_test.rs         # APIクライアント単体テスト
```

現状、`tests/` 配下には API クライアント向けの統合テストがあり、MCPツール/プロトコルレベルの統合テストは今後の拡張ポイントとする。

### 2.3 設定・認証

設定は **TOMLファイル** または **環境変数** で指定可能。環境変数が優先される:

| 環境変数 | 必須 | 説明 | 例 |
|---------|------|------|----|
| `GITBUCKET_URL` | ✅* | GitBucketインスタンスのURL | `https://gitbucket.example.com` |
| `GITBUCKET_TOKEN` | ✅* | Personal Access Token | `abc123...` |
| `GITBUCKET_MCP_CONFIG_DIR` | ❌ | 設定ディレクトリの上書き | `/custom/path` |

\* `config.toml` に未設定の場合に必須。

設定ファイルのデフォルトパス:

```text
~/.config/gitbucket-mcp-server/config.toml
```

```toml
url = "https://gitbucket.example.com"
token = "your-personal-access-token"
```

---

## 3. MCP ツール仕様

### 3.1 リポジトリツール

#### `list_repositories`
- **説明**: ユーザーまたは組織のリポジトリ一覧を取得
- **パラメータ**:
  - `owner` (string, required): ユーザー名または組織名
- **API**: `GET /api/v3/users/{owner}/repos`（404の場合 `/orgs/{owner}/repos` にフォールバック）
- **戻り値**: リポジトリ一覧（JSON配列）

#### `get_repository`
- **説明**: リポジトリの詳細情報を取得
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
- **API**: `GET /api/v3/repos/{owner}/{repo}`
- **戻り値**: リポジトリ詳細（JSON）

#### `create_repository`
- **説明**: 新しいリポジトリを作成
- **パラメータ**:
  - `name` (string, required): リポジトリ名
  - `description` (string, optional): 説明
  - `private` (boolean, optional, default: false): プライベートフラグ
  - `auto_init` (boolean, optional, default: false): README自動生成
- **API**: `POST /api/v3/user/repos`
- **戻り値**: 作成されたリポジトリ情報

#### `fork_repository`
- **説明**: リポジトリをフォーク
- **パラメータ**:
  - `owner` (string, required): フォーク元オーナー
  - `repo` (string, required): フォーク元リポジトリ名
- **API**: `POST /api/v3/repos/{owner}/{repo}/forks`
- **戻り値**: フォークされたリポジトリ情報

#### `list_branches`
- **説明**: リポジトリのブランチ一覧を取得
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
- **API**: `GET /api/v3/repos/{owner}/{repo}/branches`
- **戻り値**: ブランチ一覧

### 3.2 Issue ツール

#### `list_issues`
- **説明**: リポジトリのIssue一覧を取得
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `state` (string, optional, default: "open"): "open", "closed", "all"
- **API**: `GET /api/v3/repos/{owner}/{repo}/issues?state={state}`
- **戻り値**: Issue一覧

#### `get_issue`
- **説明**: Issue詳細を取得
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `issue_number` (integer, required): Issue番号
- **API**: `GET /api/v3/repos/{owner}/{repo}/issues/{issue_number}`
- **戻り値**: Issue詳細

#### `create_issue`
- **説明**: 新しいIssueを作成
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `title` (string, required): タイトル
  - `body` (string, optional): 本文
  - `labels` (string[], optional): ラベル名の配列
  - `assignees` (string[], optional): アサイニーの配列
- **API**: `POST /api/v3/repos/{owner}/{repo}/issues`
- **戻り値**: 作成されたIssue

#### `update_issue`
- **説明**: Issueを更新（状態変更含む）
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `issue_number` (integer, required): Issue番号
  - `state` (string, optional): "open" or "closed"
  - `title` (string, optional): 新しいタイトル
  - `body` (string, optional): 新しい本文
- **API**: `PATCH /api/v3/repos/{owner}/{repo}/issues/{issue_number}`
- **戻り値**: 更新されたIssue

#### `list_issue_comments`
- **説明**: Issueのコメント一覧を取得
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `issue_number` (integer, required): Issue番号
- **API**: `GET /api/v3/repos/{owner}/{repo}/issues/{issue_number}/comments`
- **戻り値**: コメント一覧

#### `add_issue_comment`
- **説明**: Issueにコメントを追加
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `issue_number` (integer, required): Issue番号
  - `body` (string, required): コメント本文
- **API**: `POST /api/v3/repos/{owner}/{repo}/issues/{issue_number}/comments`
- **戻り値**: 作成されたコメント

### 3.3 Pull Request ツール

#### `list_pull_requests`
- **説明**: リポジトリのPR一覧を取得
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `state` (string, optional, default: "open"): "open", "closed", "all"
- **API**: `GET /api/v3/repos/{owner}/{repo}/pulls?state={state}`
- **戻り値**: PR一覧

#### `get_pull_request`
- **説明**: PR詳細を取得
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `pull_number` (integer, required): PR番号
- **API**: `GET /api/v3/repos/{owner}/{repo}/pulls/{pull_number}`
- **戻り値**: PR詳細

#### `create_pull_request`
- **説明**: 新しいPRを作成
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `title` (string, required): タイトル
  - `head` (string, required): ヘッドブランチ名
  - `base` (string, required): ベースブランチ名
  - `body` (string, optional): 本文
- **API**: `POST /api/v3/repos/{owner}/{repo}/pulls`
- **戻り値**: 作成されたPR

#### `merge_pull_request`
- **説明**: PRをマージ
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `pull_number` (integer, required): PR番号
  - `commit_message` (string, optional): マージコミットメッセージ
- **API**: `PUT /api/v3/repos/{owner}/{repo}/pulls/{pull_number}/merge`
- **戻り値**: マージ結果

#### `add_pull_request_comment`
- **説明**: PRにコメントを追加
- **パラメータ**:
  - `owner` (string, required): リポジトリオーナー
  - `repo` (string, required): リポジトリ名
  - `pull_number` (integer, required): PR番号
  - `body` (string, required): コメント本文
- **API**: `POST /api/v3/repos/{owner}/{repo}/issues/{pull_number}/comments`（GitBucketではIssueエンドポイントを使用）
- **戻り値**: 作成されたコメント

### 3.4 ユーザーツール

#### `get_authenticated_user`
- **説明**: 認証済みユーザーの情報を取得
- **パラメータ**: なし
- **API**: `GET /api/v3/user`
- **戻り値**: ユーザー情報

#### `get_user`
- **説明**: 指定ユーザーの情報を取得
- **パラメータ**:
  - `username` (string, required): ユーザー名
- **API**: `GET /api/v3/users/{username}`
- **戻り値**: ユーザー情報

---

## 4. 実装計画（TDDアプローチ）

### Phase 0: プロジェクト初期化
- Cargoプロジェクト作成
- 依存クレート設定
- 基本的なプロジェクト構造構築
- CI設定（cargo fmt, clippy, test）

### Phase 1: 基盤層（APIクライアント + モデル + エラー型）
TDDサイクル: テスト先行で各コンポーネントを構築

1. **エラー型定義** (`error.rs`)
   - テスト: エラー型のDisplay実装、From変換
   - 実装: `GbMcpError` enum定義

2. **設定モジュール** (`config.rs`)
   - テスト: 環境変数 / TOMLファイルからの読み込み、優先順位、バリデーション
   - 実装: `Config` 構造体、`Config::load()`, `Config::load_with_file()`, `Config::from_env()`

3. **データモデル** (`models/`)
   - テスト: JSONデシリアライズ/シリアライズ（GitBucket APIレスポンスのサンプルJSONを使用）
   - 実装: User, Repository, Issue, PullRequest, Comment 各構造体

4. **APIクライアント** (`api/client.rs`)
   - テスト: wiremockによるモックHTTPテスト
     - Base URL正規化
     - 認証ヘッダー付与
     - 成功レスポンスのデシリアライズ
     - エラーレスポンスのハンドリング（401, 404, 500）
   - 実装: `GitBucketClient` 構造体、HTTP メソッド群

5. **各APIモジュール** (`api/repository.rs`, `api/issue.rs`, etc.)
   - テスト: wiremockモックで各エンドポイントのリクエスト/レスポンスを検証
   - 実装: `GitBucketClient` への各API メソッド追加

### Phase 2: MCPサーバー層
1. **サーバー骨格** (`server.rs`)
   - テスト: ServerHandler trait実装のコンパイル確認、ServerInfo返却テスト
   - 実装: `GitBucketMcpServer` 構造体、`ServerHandler` 実装

2. **ユーザーツール** (`tools/user.rs`)
   - テスト: モックAPIクライアントを使ったツール実行テスト
   - 実装: `get_authenticated_user`, `get_user`
   - ★最小ツールで全体のパイプラインを検証

3. **リポジトリツール** (`tools/repository.rs`)
   - テスト: 各ツールの入力パラメータ検証、正常/異常レスポンス
   - 実装: `list_repositories`, `get_repository`, `create_repository`, `fork_repository`, `list_branches`

4. **Issueツール** (`tools/issue.rs`)
   - テスト: CRUD操作、state変更、コメント
   - 実装: 全Issueツール

5. **Pull Requestツール** (`tools/pull_request.rs`)
   - テスト: CRUD操作、マージ、コメント
   - 実装: 全PRツール

### Phase 3: エントリポイント + 統合テスト
1. **main.rs**
   - stdio transport起動
   - TOMLファイル + 環境変数からConfig構築
   - サーバー起動

2. **統合テスト**
   - API クライアント層を `wiremock` で検証
   - MCPクライアントからのツール一覧・入力スキーマ・正常系/異常系を `tests/mcp_server_test.rs` で検証
   - 実 GitBucket に対する ignored E2E を `tests/e2e_test.rs` で追加済み
   - Docker bootstrap による再現可能な E2E 実行基盤を `scripts/e2e/` と `docker/e2e/` に追加済み
   - GitHub Actions では高速 CI と別に manual/nightly の Docker E2E workflow を運用

### Phase 4: ドキュメント + 品質向上
1. README.md（インストール方法、設定方法、使用例）
2. Claude Desktop / Copilot等でのMCPサーバー設定例
3. cargo clippy 警告ゼロ
4. エラーメッセージの改善

### Phase 5: 機能拡張ロードマップ（今後の実装）

既存の repository / issue / pull request / user ツールを壊さないため、以後の機能追加は `git worktree` で専用 branch を作成して進める。各項目は API 層、MCP ツール層、MCP 統合テスト、Docker-backed E2E、README / SPEC / TESTING の更新を同じ単位に含める。

1. **Issue メタデータ系ツールの安定化**
   - `label` と `milestone` ツールを優先して追加し、Issue 運用で頻出する分類・リリース管理を MCP から扱えるようにする。
   - REST API が 404 になる GitBucket 互換差異は、対象リソースの存在確認後に限定して web fallback を使う。
   - E2E では `create -> get/list -> update -> delete` の lifecycle を専用テストデータで検証し、再実行時の衝突を避ける。

2. **Issue 更新機能の拡張**
   - `update_issue` を `state/title/body` だけでなく、`labels`, `assignees`, `milestone` の更新に広げる。
   - REST 非対応項目は、fallback 可否を項目ごとに明示し、未対応の場合は structured MCP error で返す。
   - 既存の issue write-path E2E にメタデータ更新の確認を追加する。

3. **Pull Request 補助機能**
   - `list_pull_request_comments` を追加し、PR の会話履歴を MCP から取得できるようにする。
   - 必要に応じて PR の close/reopen 相当を追加する。ただし GitBucket API 互換性を先に確認し、web fallback が必要な場合は Issue と同じ存在確認ルールを適用する。

4. **検索・探索系ツール**
   - `search_issues`, `search_repositories`, `search_pull_requests` を候補とする。
   - GitBucket REST API の対応範囲に差が出やすいため、実装前に GitBucket 4.44.0 と利用中インスタンスでエンドポイントを確認する。

5. **高度な repository / multi-user E2E**
   - `fork_repository` の E2E は、複数ユーザーと fork 元 repo の bootstrap が必要なため後続に回す。
   - `delete_repository` など破壊的な管理操作は、明確な需要が出るまで実装しない。追加する場合は tool 名、ドキュメント、E2E で危険操作であることを明示する。

---

## 5. テスト戦略

### 5.1 テストレベル

| レベル | ツール | 対象 |
|--------|--------|------|
| 単体テスト | `#[cfg(test)]` | モデルのシリアライズ、設定バリデーション、URL正規化 |
| APIモックテスト | `wiremock` | APIクライアントのHTTPリクエスト/レスポンス |
| ツールテスト | `src/tools/*` + モックAPI | MCPツールハンドラーの入力検証と成功系 |
| MCP統合テスト | `tests/mcp_server_test.rs` | MCPプロトコル経由のツール列挙・schema・structured success/error 呼び出し |
| E2Eテスト | `tests/e2e_test.rs` | 実 GitBucket に対する read/write smoke test |

### 5.2 TDDワークフロー

各機能について以下のサイクルを繰り返す:

1. **Red**: 失敗するテストを書く
2. **Green**: テストを通す最小限の実装
3. **Refactor**: コードを整理（テストは通ったまま）

### 5.3 テストデータ

GitBucket APIレスポンスのサンプルJSONをテストフィクスチャとして使用。
`gitbucket-cli-rs` のテストコードから実際のレスポンス形式を参照可能。

---

## 6. 使用例

### 6.1 MCPサーバー起動

```bash
# オプション1: 環境変数設定
export GITBUCKET_URL="https://gitbucket.example.com"
export GITBUCKET_TOKEN="your-personal-access-token"

# サーバー起動（stdioモード）
gitbucket-mcp-server
```

```bash
# オプション2: ~/.config/gitbucket-mcp-server/config.toml を使用
gitbucket-mcp-server
```

### 6.2 Claude Desktop設定例

```json
{
  "mcpServers": {
    "gitbucket": {
      "command": "gitbucket-mcp-server",
      "env": {
        "GITBUCKET_URL": "https://gitbucket.example.com",
        "GITBUCKET_TOKEN": "your-token"
      }
    }
  }
}
```

### 6.3 ツール呼び出し例

AIモデルが以下のようにツールを呼び出す:

```json
{
  "tool": "list_issues",
  "arguments": {
    "owner": "myuser",
    "repo": "myproject",
    "state": "open"
  }
}
```

---

## 7. 備考

- GitBucket API は GitHub API v3 のサブセットであるため、一部のエンドポイントは利用できない可能性がある。`gitbucket-cli-rs` で動作確認済みのエンドポイントを優先的に実装する。
- list 系エンドポイントは `per_page=100` で自動 pagination し、短い最終ページまで全件収集する。
- `update_issue(state/title/body)` は、REST API の `PATCH` が 404 でも対象 Issue の `GET` が成功した場合に限って optional な Web Session フォールバックを使用する。
- ログ出力は `tracing` クレートで stderr に出力する（stdio transport を使用するため、stdout はMCPプロトコル通信に使用）。
