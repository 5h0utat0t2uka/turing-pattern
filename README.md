# turing-pattern

Gray–Scott反応拡散モデルでチューリングパターンを生成し、[chafa](https://hpjansson.org/chafa/) で表示・保存するCLI  

![demo](./img/ss.png)  

## 使い方
```sh
nix run .                                  # 512×512、すべてランダム
nix run . -- --size 256 --pattern worms    # サイズと模様を指定
nix run . -- --seed 42                     # 同じ模様・配色を再現
nix run . -- --text alice@example.com      # 文字列から決定（Identicon風）
nix run . -- --all                         # 全種類を並べて表示
nix run . -- --text alice --save dist      # 表示しつつ dist/ にPNGで保存
```

| 引数 | 既定値 | 内容 |
| --- | --- | --- |
| `--size` | `512`（`--all`では`256`） | 一辺のピクセル数。64未満は64で計算して縮小 |
| `--seed` | ランダム | 0〜2⁶⁴−1。模様の種類・配色・形がすべて決まる |
| `--text` | なし | 文字列からseedを決める。`--seed`とは併用不可 |
| `--steps` | `4000` | シミュレーションの更新回数 |
| `--pattern` | seedで決定 | 模様の種類（下表） |
| `--all` | なし | 全種類を同じseed・配色で横4列に並べて表示。`--pattern`とは併用不可 |
| `--save [DIR]` | 保存しない | 表示に加えてPNGを保存。DIR省略時は現在のディレクトリ |

seed・模様の種類・配色・画像idは標準エラー出力に表示される

### 画像idと保存
- 画像ごとに12桁のidを持つ。生成方法のバージョン・模様（`--all`では`all`）・seed・サイズ・ステップ数をまとめたSHA-256の先頭12桁
- 同じ`--text`と同じ設定なら同じid、どれかが違えば違うid。生成方法を変えたら`id.rs`のバージョンを上げ、以前のidと重ならないようにする
- 保存時のファイル名は`turing-<id>.png`。同じ画像をもう一度保存しても複製は作らない（`Already saved`と表示）
- PNGの中に元の値を記録する（例: `turing-pattern v1 pattern=maze seed=7 size=24 steps=30 id=8535ba4f27f3`）。`--text`の文字列は記録しない
- 既存ファイルは上書きしない。同じ名前で中身が違う場合は`-2`・`-3`…を付けて保存
- 書き込み途中のファイルが最終的な名前で残ることはない

### 文字列からの生成
- 同じ文字列・`--size`・`--steps`であれば決定論的に同じ画像になる
- seedは`turing-pattern:v1:`＋文字列のSHA-256の先頭8バイト（リトルエンディアン）
- 文字列はUnicodeのNFC正規化のみ行う。見た目が同じ文字（合成済みの「が」と「か」＋濁点など）は同じ画像になり、大文字・小文字や空白は区別する
- 画像が変わらないことを`tests/golden.rs`で固定している。生成方法を変える場合は`v1`を上げる

## 模様の種類
| 種類 | 見た目 | F | k |
| --- | --- | --- | --- |
| `coral` | 珊瑚状に曲がりくねる帯 | 0.0545 | 0.062 |
| `maze` | 細かく入り組んだ迷路 | 0.029 | 0.057 |
| `spots` | 等間隔に並ぶ斑点 | 0.0367 | 0.0649 |
| `holes` | 斑点を反転したような穴 | 0.039 | 0.058 |
| `worms` | 離れて伸びる短い線 | 0.058 | 0.065 |
| `dashes` | 点と短い線が密に混ざる | 0.046 | 0.065 |
| `loops` | 輪と点が網目状につながる | 0.062 | 0.061 |

## 開発
Nix（flakes有効）・direnv・nix-direnvを使用
```sh
direnv allow
cargo run --release -- --size 256    # 作業ツリーを直接実行
```

テスト
```sh
cargo test --release
cargo run --release --example bench    # サイズごとの計算時間
nix flake check                         # ビルド時のテストと実行確認
```

## 仕組み
- 濃度`u`・`v`を前進Euler法（Δt=1）で更新。拡散係数は`Du=1.0`・`Dv=0.5`、Laplacianは9点ステンシル、境界は周期境界
- Rustで実装。2つのバッファを交互に使い、端以外はSIMDで計算できる形にしている
- 一辺256ピクセル以上ではrayonで行ごとに複数のCPUコアへ分配
- 高速版の結果は、テスト内の素直な参照実装とビット単位で一致
- 模様の種類・配色・初期配置はseedから別々の乱数（xoshiro256++）で決めるので、互いに影響しない
- 模様1つ分は10〜20ピクセルほどあり、小さな格子では模様が消えて単色になりやすい。そのため64ピクセル未満は64ピクセルで計算し、面積平均で縮小する（同じseedなら16・32・48ピクセルは同じ図柄の縮小になる）

| ファイル | 内容 |
| --- | --- |
| `src/sim.rs` | シミュレーションと模様の種類 |
| `src/render.rs` | 配色・描画・縮小・PNG変換 |
| `src/text.rs` | 文字列からのseed |
| `src/id.rs` | 画像id |
| `src/save.rs` | 保存とファイル名 |
| `src/lib.rs` | 計算から画像までの流れ |
| `src/display.rs` | chafaでの表示 |
| `src/rng.rs` | 乱数 |
| `src/main.rs` | 引数の処理 |

計算時間の目安（Apple Silicon 10コア、4000ステップ）
| 一辺 | 時間 |
| --- | --- |
| 256 | 0.13秒 |
| 512 | 0.42秒 |
| 1024 | 0.7秒 |
| 2048 | 4.4秒 |

## 参考資料
- [Gray–Scottモデル — MIT CSAIL](https://groups.csail.mit.edu/mac/projects/amorphous/GrayScott/)
- [Reaction-Diffusion Tutorial — Karl Sims](https://karlsims.com/rd.html)
- [xoshiro / xorshift generators](https://prng.di.unimi.it/)
- [Rayon](https://docs.rs/rayon)
