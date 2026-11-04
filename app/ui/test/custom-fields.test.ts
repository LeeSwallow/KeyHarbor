// Fields the person names on every kind, each secret or plain, and credential templates
// (R-48, #205): the item forms, the settings section, and what the forms send.

import { describe, expect, it } from "vitest";
import type { Item, KindForm, Template } from "../src/api.js";
import { editor, editorValues } from "../src/views/editor.js";
import { form, formValues } from "../src/views/form.js";
import { settings, templateForm } from "../src/views/settings.js";

const credential: KindForm = {
  kind: "credential", domain: "credentials", free_fields: false, custom_fields: true, generates: null,
  fields: [{ name: "note", secret: true }],
};
const login: KindForm = {
  kind: "login", domain: "credentials", free_fields: false, custom_fields: true, generates: "password",
  fields: [{ name: "username", secret: false }, { name: "password", secret: true }],
};
const bank: Template = { name: "bank", fields: [{ name: "second_password", secret: true }, { name: "member_no", secret: false }] };

describe("the new item form", () => {
  it("lets a login take a field of its own, secret unless 일반", () => {
    const html = editor({ mode: "new", kind: "login" }, [login], []);
    expect(html).toContain("직접 추가한 항목");
    expect(html).toContain('name="cv:0" type="password"');
    expect(html).toContain('name="cp:0"');
    expect(html).not.toContain('name="cp:0" checked');
  });

  it("offers templates for a credential and fills its rows from the one picked", () => {
    const none = editor({ mode: "new", kind: "credential" }, [credential], [], [bank]);
    expect(none).toContain('data-action="pick-template"');
    expect(none).toContain('<option value="bank">bank</option>');
    const picked = editor({ mode: "new", kind: "credential", template: "bank" }, [credential], [], [bank]);
    expect(picked).toContain('name="cn:0" placeholder="이름 (예: school)" value="second_password"');
    expect(picked).toContain('name="cv:0" type="password"');
    expect(picked).toContain('name="cn:1" placeholder="이름 (예: school)" value="member_no"');
    expect(picked).toContain('name="cv:1" type="text"');
    expect(picked).toContain('name="cp:1" checked');
    expect(editor({ mode: "new", kind: "login" }, [login], [], [bank])).not.toContain("pick-template");
  });

  it("sends named rows with the plain ones listed and refuses a name the form has", () => {
    const data = new FormData();
    data.set("v:password", "pw-SECRET");
    data.set("cn:0", "second_password");
    data.set("cv:0", "2nd-SECRET");
    data.set("cn:1", "member_no");
    data.set("cv:1", "M-1");
    data.set("cp:1", "on");
    data.set("cn:2", "empty_plain");
    data.set("cv:2", "");
    data.set("cp:2", "on");
    const sent = editorValues(data);
    expect(sent.values).toEqual({ password: "pw-SECRET", second_password: "2nd-SECRET", member_no: "M-1" });
    expect(sent.plain).toEqual(["member_no"]);
    expect(sent.clashes).toEqual([]);
    data.set("cn:3", "password");
    data.set("cv:3", "other");
    expect(editorValues(data).clashes).toEqual(["password"]);
    expect(editorValues(data).values.password).toBe("pw-SECRET");
  });
});

describe("changing an item's own fields", () => {
  const item: Item = {
    name: "hana", kind: "credential", domain: "credentials", status: "active", source: "user", description: null,
    fields: ["member_no", "second_password"], placeholders: [], plain_values: { member_no: "M-1" }, origins: [], tags: [],
    expired: false, tampered: false,
  };

  it("shows each own field with a plain box and a remove box, and no secret value", () => {
    const html = editor({ mode: "edit", name: "hana" }, [credential], [item]);
    expect(html).toContain('name="pl:member_no" checked');
    expect(html).toContain('name="pw:member_no"');
    expect(html).toContain('name="pl:second_password">');
    expect(html).toContain('name="rm:second_password"');
    expect(html).toContain('name="v:second_password" type="password"');
  });

  it("sends what was marked plain or secret again, and what was taken out", () => {
    const data = new FormData();
    data.set("v:member_no", "");
    data.set("pw:member_no", "1");
    data.set("v:second_password", "");
    data.set("pl:second_password", "on");
    data.set("v:note", "");
    data.set("rm:note", "on");
    const sent = editorValues(data);
    expect(sent.plain).toEqual(["second_password"]);
    expect(sent.secret).toEqual(["member_no"]);
    expect(sent.remove).toEqual(["note"]);
  });
});

describe("the secure input form for a credential", () => {
  const request = {
    id: "r1", agent: "codex", kind: "credential", name: "hana", origin: null, reason: "2차 비밀번호",
    fields: [{ name: "note", secret: true }], free_fields: false, custom_fields: true,
  };

  it("takes fields the person names, always as password inputs", () => {
    const html = form(request);
    expect(html).toContain('name="cn:0"');
    expect(html).toContain('name="cv:0" type="password"');
    const data = new FormData();
    data.set("note", "");
    data.set("cn:0", "second_password");
    data.set("cv:0", "2nd-SECRET");
    data.set("cn:1", "note");
    data.set("cv:1", "would replace");
    expect(formValues(data)).toEqual({ note: "", second_password: "2nd-SECRET" });
  });
});

describe("the settings screen's templates", () => {
  it("lists templates with their plain fields marked and a remove button", () => {
    const html = settings([], undefined, null, [bank]);
    expect(html).toContain("<h3>자격 증명 틀</h3>");
    expect(html).toContain("<strong>bank</strong>");
    expect(html).toContain("member_no (일반)");
    expect(html).toContain('data-action="remove-template" data-name="bank"');
    expect(html).toContain("data-template-form");
    expect(settings([])).toContain("만든 틀이 없습니다");
  });

  it("sends a template's secret fields, then its plain ones", () => {
    const data = new FormData();
    data.set("name", " bank ");
    data.set("secret", "second_password\n\n");
    data.set("plain", " member_no ");
    expect(templateForm(data)).toEqual(bank);
  });
});
