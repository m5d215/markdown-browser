---
title: Markdown Browser Front Matter
date: 2026-05-12
tags: [rust, markdown, terminal]
draft: false
description: |
  Front matter rendering smoke test.
  Lines with multiline values keep their indentation.
author:
  name: m5d215
  email: noreply@example.com
---

# Front Matter Showcase

front matter は YAML (`---`) と TOML (`+++`) の両方を検出して、文書冒頭にパネルとして描画する。

## キーと値の区切り

`key: value` (YAML) と `key = value` (TOML) を検出して、キーを強調表示する。

## ネスト構造

ネストされた値はそのままインデント込みで表示する。複雑な値（リスト、マップ）は元の表記のまま見せる方針。
