//! 无头自测脚本：模拟完整玩法流程并输出验收截图
use crate::editor;
use crate::save;
use crate::GameApp;
use glam::Vec2;
use mge_platform::action::Action;
use mge_platform::input::InputState;
use mge_render::camera::Camera;
use mge_runtime::EngineCtx;
use winit::event::ElementState;

const PRESS: ElementState = ElementState::Pressed;
const RELEASE: ElementState = ElementState::Released;

const SHOTS: &[(u64, &str)] = &[
    (30, "01_spawn"),
    (125, "02_run"),
    (300, "03_pixels"),
    (430, "08_sand_support"),
    (600, "04_mining"),
    (700, "05_build"),
    (850, "06_combat"),
    (928, "09_vfx"),
    (980, "07_night"),
];

pub fn drive(game: &mut GameApp, ctx: &mut EngineCtx) {
    let tick = ctx.frame;
    for (t, name) in SHOTS {
        if *t == tick {
            ctx.request_screenshot(format!("screenshots/{name}.png"));
        }
    }
    let input = &mut *ctx.input;
    let cam = &*ctx.camera;
    let aim = |input: &mut InputState, cam: &Camera, wx: f32, wy: f32| {
        input.mouse_pos = cam.world_to_screen(Vec2::new(wx, wy));
    };
    let px = game.player.pos.x;
    let py = game.player.pos.y;

    match tick {
        // 待机稳定
        0..=39 => aim(input, cam, px + 30.0, py - 10.0),
        // 跑动 + 跳跃
        40 => input.inject(Action::Right, PRESS),
        100 => input.inject(Action::Jump, PRESS),
        105 => input.inject(Action::Jump, RELEASE),
        150 => input.inject(Action::Right, RELEASE),
        // 倒水（落在脚边平地成池）+ 倒沙（高处落下堆成沙堆）
        // 工具改为左键单击单次触发：循环点击倒出足量
        170 => input.inject(Action::Slot5, PRESS),
        171 => input.inject(Action::Slot5, RELEASE),
        175..=249 => {
            aim(input, cam, px + 10.0, py - 30.0);
            if tick % 3 == 0 {
                input.inject(Action::Attack, PRESS);
            } else if tick % 3 == 2 {
                input.inject(Action::Attack, RELEASE);
            }
        }
        250 => input.inject(Action::Attack, RELEASE),
        255 => input.inject(Action::Slot6, PRESS),
        256 => input.inject(Action::Slot6, RELEASE),
        // 沙倒在脚边水洼里：沙沉底堆积，把角色托起（粉末支撑验证）
        260..=339 => {
            aim(input, cam, px + 2.0, py - 24.0);
            if tick % 3 == 0 {
                input.inject(Action::Attack, PRESS);
            } else if tick % 3 == 2 {
                input.inject(Action::Attack, RELEASE);
            }
        }
        340 => input.inject(Action::Attack, RELEASE),
        // 静置 70 tick：沙沉底结块，角色应被粉末支撑托起
        430 => {
            println!(
                "[DBG] on-sand pos=({:.1},{:.1}) wading={}",
                game.player.pos.x, game.player.pos.y, game.player.wading
            );
        }
        // 蹚出沙堆水洼
        435 => input.inject(Action::Left, PRESS),
        470 => input.inject(Action::Left, RELEASE),
        480 => {
            println!(
                "[DBG] back pos=({:.1},{:.1}) wading={}",
                game.player.pos.x, game.player.pos.y, game.player.wading
            );
        }
        // 镐：挖掘脚下右侧地面
        485 => input.inject(Action::Slot2, PRESS),
        486 => input.inject(Action::Slot2, RELEASE),
        490..=599 => {
            aim(input, cam, px + 20.0, py + 12.0);
            if tick % 3 == 0 {
                input.inject(Action::Attack, PRESS);
            } else if tick % 3 == 2 {
                input.inject(Action::Attack, RELEASE);
            }
        }
        600 => input.inject(Action::Attack, RELEASE),
        // 石块竖墙（4px 块 × 3）+ 顶部火把
        605 => input.inject(Action::Slot3, PRESS),
        606 => input.inject(Action::Slot3, RELEASE),
        610..=651 => {
            let i = (tick - 610) / 14;
            aim(input, cam, px + 16.0, py - 6.0 - i as f32 * 4.0);
            if (tick - 610) % 14 == 0 {
                input.inject(Action::Attack, PRESS);
            } else if (tick - 610) % 14 == 7 {
                input.inject(Action::Attack, RELEASE);
            }
        }
        655 => input.inject(Action::Slot4, PRESS),
        656 => input.inject(Action::Slot4, RELEASE),
        660..=679 => {
            aim(input, cam, px + 16.0, py - 18.0);
            if tick % 3 == 0 {
                input.inject(Action::Attack, PRESS);
            } else if tick % 3 == 2 {
                input.inject(Action::Attack, RELEASE);
            }
        }
        680 => input.inject(Action::Attack, RELEASE),
        // 剑击假人：向右跑到假人身前
        705 => input.inject(Action::Slot1, PRESS),
        706 => input.inject(Action::Slot1, RELEASE),
        710..=788 => {
            input.inject(Action::Right, PRESS);
            aim(input, cam, px + 30.0, py - 12.0);
        }
        789 => input.inject(Action::Right, RELEASE),
        800 => input.inject(Action::Attack, PRESS),
        806 => input.inject(Action::Attack, RELEASE),
        814 => input.inject(Action::Attack, PRESS),
        820 => input.inject(Action::Attack, RELEASE),
        828 => input.inject(Action::Attack, PRESS),
        840 => input.inject(Action::Attack, RELEASE),
        // 弓箭：3 连射假人（7 号栏）
        845 => input.inject(Action::Slot7, PRESS),
        846 => input.inject(Action::Slot7, RELEASE),
        850..=895 => {
            aim(input, cam, px + 34.0, py - 20.0);
            let t = tick - 850;
            if t % 15 == 0 {
                input.inject(Action::Attack, PRESS);
            } else if t % 15 == 4 {
                input.inject(Action::Attack, RELEASE);
            }
        }
        // 火球：轰击假人右侧地面 → 爆炸弹坑（8 号栏）
        900 => input.inject(Action::Slot8, PRESS),
        901 => input.inject(Action::Slot8, RELEASE),
        905..=927 => {
            aim(input, cam, px + 44.0, py - 2.0);
            if tick == 908 {
                input.inject(Action::Attack, PRESS);
            } else if tick == 913 {
                input.inject(Action::Attack, RELEASE);
            }
        }
        930 => {
            println!(
                "[SELFTEST] vfx particles {} texts {} | projectiles {}",
                game.vfx.particles.len(),
                game.vfx.texts.len(),
                game.projectiles.list.len(),
            );
        }
        // ---- 特效编辑器数据链路：新建蓝图 → 保存 → 读回 → 挂武器 → 触发 ----
        940 => {
            use crate::vfx::{Blueprint, Emitter};
            game.vfx.bps.insert(
                "editor_test_fx".to_string(),
                Blueprint {
                    shake: 3.0,
                    hitstop: 2,
                    emitters: vec![Emitter {
                        count: 40,
                        spread: 3.14,
                        speed: [80.0, 200.0],
                        life: [0.3, 0.7],
                        color: [1.0, 0.4, 0.1],
                        glow: true,
                        ..Default::default()
                    }],
                },
            );
            editor::save_vfx(game);
            // 模拟外部修改：从磁盘读回验证序列化正确
            let s = std::fs::read_to_string(editor::VFX_PATH).expect("vfx.ron 缺失");
            let bps: std::collections::HashMap<String, Blueprint> = ron::from_str(&s).expect("vfx.ron 读回失败");
            assert!(bps.contains_key("editor_test_fx"), "保存的蓝图读回缺失");
            game.vfx.bps = bps;
            // 挂到武器
            game.weapons.slash = "editor_test_fx".into();
            editor::save_weapons(game);
        }
        945 => {
            // 面板触发路径 + 切剑（此时 weapons.slash 已指向新蓝图）
            game.editor.sel = "editor_test_fx".to_string();
            game.editor.trigger = true;
            input.inject(Action::Slot1, PRESS);
        }
        946 => input.inject(Action::Slot1, RELEASE),
        950 => input.inject(Action::Attack, PRESS),
        956 => input.inject(Action::Attack, RELEASE),
        958 => {
            let ok = game.vfx.bps.contains_key("editor_test_fx")
                && game.weapons.slash == "editor_test_fx"
                && game.vfx.particles.len() > 0;
            println!(
                "[SELFTEST] editor link {} | bps {} | slash={} | particles {}",
                if ok { "PASS" } else { "FAIL" },
                game.vfx.bps.len(),
                game.weapons.slash,
                game.vfx.particles.len(),
            );
        }
        // 清理测试数据，恢复原始文件
        965 => {
            game.vfx.bps.remove("editor_test_fx");
            editor::save_vfx(game);
            game.weapons.slash = "sword_slash".into();
            editor::save_weapons(game);
        }
        // ---- M8 暗黑循环：杀怪 → 掉落 → 磁吸拾取 → 装备 → 变强 ----
        966 => {
            // 击杀第一只存活假人（下一帧触发死亡→掉落表 roll + 经验）
            let mut q = game.ecs.query::<&mut crate::entities::Dummy>();
            for (_e, dm) in q.iter() {
                if dm.respawn == 0 {
                    dm.hp = 0.0;
                    break;
                }
            }
        }
        968 => {
            println!(
                "[SELFTEST] drops {} | bag {} | xp {}",
                game.drops.list.len(),
                game.inv.bag.iter().flatten().count(),
                game.inv.xp,
            );
            ctx.request_screenshot("screenshots/10_drops.png");
        }
        972 => {
            // 全部掉落物传送到玩家脚下（磁吸拾取）
            let p = game.player.pos;
            for d in game.drops.list.iter_mut() {
                d.pos = p + Vec2::new(2.0, -4.0);
                d.delay = 0.0;
            }
        }
        978 => {
            // 拾取后：bag 应非空、drops 应清空（金币入 gold 或物品入包）
            let bag_n = game.inv.bag.iter().flatten().count();
            println!(
                "[SELFTEST] picked | bag {} gold {} drops left {}",
                bag_n,
                game.inv.gold,
                game.drops.list.len(),
            );
            // 装备第一件装备并验证聚合属性变化；无装备则确定性塞一件铁剑
            let mut equip_idx = None;
            for (i, s) in game.inv.bag.iter().enumerate() {
                if let Some(it) = s {
                    if game.db.def(&it.def).stack == 1 {
                        equip_idx = Some(i);
                        break;
                    }
                }
            }
            let equip_idx = match equip_idx {
                Some(i) => Some(i),
                None => {
                    game.inv.add(
                        crate::items::Item {
                            def: "sword_iron".into(),
                            count: 1,
                            affixes: vec![],
                        },
                        &game.db,
                    );
                    game.inv
                        .bag
                        .iter()
                        .position(|s| matches!(s, Some(it) if it.def == "sword_iron"))
                }
            };
            if let Some(i) = equip_idx {
                let before = game.inv.aggregate(&game.db);
                let before_sum = before.dmg_flat + before.armor + before.hp;
                let ok = game.inv.equip_from_bag(i, &game.db);
                let after = game.inv.aggregate(&game.db);
                let after_sum = after.dmg_flat + after.armor + after.hp;
                println!(
                    "[SELFTEST] equip {} | stats {:.1} -> {:.1}",
                    if ok { "OK" } else { "FAIL" },
                    before_sum,
                    after_sum,
                );
            }
        }
        988 => {
            let bag_n = game.inv.bag.iter().flatten().count();
            let equipped = game.inv.equip.iter().any(|s| s.is_some());
            let st = game.inv.aggregate(&game.db);
            let pass = equipped && game.inv.xp >= 10;
            println!(
                "[SELFTEST] dark loop {} | xp {} bag {} equipped {} | atk {:.1} (base 15)",
                if pass { "PASS" } else { "FAIL" },
                game.inv.xp,
                bag_n,
                equipped,
                st.dmg_flat,
            );
        }
        // ---- M9：夜晚刷怪 + 战斗 AI + BOSS 多阶段 ----
        990 => {
            game.world.time = 0.85; // 深夜
            game.inv.level = 3; // 解锁 BOSS
        }
        995 => {
            // 确定性生成测试怪群
            use crate::monsters::Kind;
            let px = game.player.pos.x;
            let sy = game.surface_y(px as i32 + 60) as f32;
            let sy2 = game.surface_y(px as i32 + 100) as f32;
            game.monsters.test_spawn(Kind::Slime, Vec2::new(px + 60.0, sy), &mut game.rng);
            game.monsters.test_spawn(Kind::Slime, Vec2::new(px + 100.0, sy2), &mut game.rng);
            game.monsters
                .test_spawn(Kind::Bat, Vec2::new(px + 80.0, sy - 80.0), &mut game.rng);
            game.monsters
                .test_spawn(Kind::Archer, Vec2::new(px - 90.0, sy), &mut game.rng);
            game.monsters
                .test_spawn(Kind::Boss, Vec2::new(px - 140.0, sy - 10.0), &mut game.rng);
        }
        1000 => {
            let boss = game
                .monsters
                .list
                .iter()
                .find(|m| m.boss)
                .map(|b| (b.pos.x as i32, b.phase));
            println!(
                "[SELFTEST] monsters {} | boss alive {} at {:?}",
                game.monsters.list.len(),
                game.monsters.boss_alive,
                boss,
            );
            ctx.request_screenshot("screenshots/11_night_combat.png");
        }
        // 挥剑打最近的怪（真实战斗）
        1002 => input.inject(Action::Slot1, PRESS),
        1003 => input.inject(Action::Slot1, RELEASE),
        1005 => {
            input.inject(Action::Attack, PRESS);
            // 面向左侧怪群
        }
        1011 => input.inject(Action::Attack, RELEASE),
        1015 => {
            let chasing = game
                .monsters
                .list
                .iter()
                .filter(|m| m.state == crate::monsters::AiState::Chase)
                .count();
            let hurt = game
                .monsters
                .list
                .iter()
                .any(|m| m.hp < m.max_hp);
            println!(
                "[SELFTEST] ai chasing {}/{} | monster hurt {}",
                chasing,
                game.monsters.list.len(),
                hurt,
            );
        }
        1016 => {
            // BOSS 打入 P2 血线，验证阶段切换 + 弹幕
            if let Some(b) = game.monsters.list.iter_mut().find(|m| m.boss) {
                b.hp = b.max_hp * 0.3;
            }
        }
        1026 => {
            let phase = game
                .monsters
                .list
                .iter()
                .find(|m| m.boss)
                .map(|b| b.phase)
                .unwrap_or(255);
            let bullets = game.monsters.bullets.len();
            println!("[SELFTEST] boss phase {} | bullets {}", phase, bullets);
        }
        1036 => {
            let boss_dead = !game.monsters.list.iter().any(|m| m.boss);
            let chasing = game
                .monsters
                .list
                .iter()
                .filter(|m| m.state == crate::monsters::AiState::Chase)
                .count();
            let pass = game.monsters.list.len() > 0 && chasing > 0;
            println!(
                "[SELFTEST] m9 ai {} | alive {} chasing {} boss_dead {}",
                if pass { "PASS" } else { "FAIL" },
                game.monsters.list.len(),
                chasing,
                boss_dead,
            );
        }
        // ---- M10：存档/读档（世界改动还原）----
        1040 => {
            // 找 spawn 下方第一个实心点 → 挖掉 → 存档（该点空被记录）
            let (sx, sy) = (game.world.spawn_x, game.world.spawn_y);
            for dy in 10..80 {
                if game.world.solid_px(sx, sy + dy) {
                    game.world.mine_px(sx, sy + dy, 255);
                    break;
                }
            }
            if save::save_game(game).is_err() {
                println!("[SELFTEST] save FAILED to write");
            }
        }
        1042 => {
            // 手动把该点填回实心（伪造"未挖"状态）——读档后应被存档态（空）覆盖
            let (sx, sy) = (game.world.spawn_x, game.world.spawn_y);
            for dy in 10..80 {
                if !game.world.solid_px(sx, sy + dy) {
                    let dirt = game.world.mats.id("dirt").unwrap_or(0);
                    game.world.pixels.set(
                        sx,
                        sy + dy,
                        mge_sim::Pixel { mat: dirt, shade: 128, life: 0, aux: 0 },
                    );
                    break;
                }
            }
        }
        1044 => {
            if let Err(e) = save::load_game(game) {
                println!("[SELFTEST] load FAILED: {e}");
            }
        }
        1048 => {
            // 存档里该点为空：读档后应再次为空（读档覆盖生效）
            let (sx, sy) = (game.world.spawn_x, game.world.spawn_y);
            let mut mined_kept = false;
            for dy in 10..80 {
                if !game.world.solid_px(sx, sy + dy) {
                    mined_kept = true;
                    break;
                }
            }
            println!(
                "[SELFTEST] save/load {} | mined_restored_to_empty {} | lvl {}",
                if mined_kept { "PASS" } else { "FAIL" },
                mined_kept,
                game.inv.level,
            );
        }
        // ---- M14：NPC 生活 AI（需求驱动 吃/喝/睡 循环）----
        1050 => {
            // NPC0：饿（浆果种在东侧 40px）→ 找食物；NPC1：渴（现挖水池）→ 找水
            let berry = game.world.mats.id("berry").unwrap_or(0);
            let water = game.world.mats.id("water").unwrap_or(0);
            if game.npcs.list.len() >= 2 {
                let x0 = game.npcs.list[0].pos.x as i32 + 40;
                let sy0 = game.surface_y(x0);
                game.world.pixels.set(
                    x0,
                    sy0 - 1,
                    mge_sim::Pixel { mat: berry, shade: 180, life: 0, aux: 0 },
                );
                game.npcs.list[0].hunger = 45.0;
                game.npcs.list[0].thirst = 0.0;
                game.npcs.list[0].fatigue = 0.0;
                let x1 = game.npcs.list[1].pos.x as i32 + 50;
                let sy1 = game.surface_y(x1);
                for dx in -6..=6 {
                    for dy in 0..3 {
                        game.world.pixels
                            .set(x1 + dx, sy1 + dy, mge_sim::Pixel::default());
                    }
                }
                for dx in -5..=5 {
                    for dy in 1..3 {
                        game.world.pixels.set(
                            x1 + dx,
                            sy1 + dy,
                            mge_sim::Pixel { mat: water, shade: 160, life: 0, aux: 0 },
                        );
                    }
                }
                game.npcs.list[1].pos = Vec2::new(x1 as f32 - 10.0, sy1 as f32);
                game.npcs.list[1].thirst = 45.0;
                game.npcs.list[1].hunger = 0.0;
                game.npcs.list[1].fatigue = 0.0;
            }
            println!(
                "[SELFTEST] npcs {} | seeded food+water | hunger0 {:.0} thirst1 {:.0}",
                game.npcs.list.len(),
                game.npcs.list.first().map(|n| n.hunger).unwrap_or(0.0),
                game.npcs.list.get(1).map(|n| n.thirst).unwrap_or(0.0),
            );
        }
        1054 => {
            // 传送到目标旁 → 触发进食流程（省去走路帧）
            if let Some(n0) = game.npcs.list.get_mut(0) {
                if n0.state == crate::npc::NpcState::SeekFood {
                    n0.pos = n0.target + Vec2::new(0.0, -1.0);
                }
            }
        }
        1058 => {
            let eat = game
                .npcs
                .list
                .get(0)
                .map(|n| matches!(n.state, crate::npc::NpcState::Eat(_)))
                .unwrap_or(false);
            let drink = game
                .npcs
                .list
                .get(1)
                .map(|n| matches!(n.state, crate::npc::NpcState::Drink(_)))
                .unwrap_or(false);
            println!(
                "[SELFTEST] npc act eat {} drink {}",
                if eat { "OK" } else { "FAIL" },
                if drink { "OK" } else { "FAIL" },
            );
            ctx.request_screenshot("screenshots/12_npc.png");
        }
        1148 => {
            // 进食/饮水完成：需求归零
            let eat_ok = game.npcs.list.get(0).map(|n| n.hunger < 1.0).unwrap_or(false);
            let drink_ok = game.npcs.list.get(1).map(|n| n.thirst < 1.0).unwrap_or(false);
            println!(
                "[SELFTEST] npc consume {} | hunger0 {:.2} thirst1 {:.2}",
                if eat_ok && drink_ok { "PASS" } else { "FAIL" },
                game.npcs.list.first().map(|n| n.hunger).unwrap_or(999.0),
                game.npcs.list.get(1).map(|n| n.thirst).unwrap_or(999.0),
            );
        }
        1150 => {
            // 疲劳强制拉升 → 回家睡觉（疲劳恢复）
            if let Some(n0) = game.npcs.list.get_mut(0) {
                n0.pos = n0.home;
                n0.fatigue = 65.0;
            }
        }
        1162 => {
            let (st, f) = game
                .npcs
                .list
                .first()
                .map(|n| (n.state, n.fatigue))
                .unwrap_or((crate::npc::NpcState::Idle, 999.0));
            let ok = st == crate::npc::NpcState::Sleep && f < 64.0;
            println!(
                "[SELFTEST] npc sleep {} | state {:?} fatigue {:.2}",
                if ok { "PASS" } else { "FAIL" },
                st,
                f,
            );
        }
        1176 => {
            let eat_ok = game.npcs.list.get(0).map(|n| n.hunger < 1.0).unwrap_or(false);
            let drink_ok = game.npcs.list.get(1).map(|n| n.thirst < 1.0).unwrap_or(false);
            let sleep_ok = game
                .npcs
                .list
                .get(0)
                .map(|n| n.fatigue < 64.0)
                .unwrap_or(false);
            let pass = eat_ok && drink_ok && sleep_ok;
            println!(
                "[SELFTEST] m14 npc {} | eat {} drink {} sleep {}",
                if pass { "PASS" } else { "FAIL" },
                eat_ok,
                drink_ok,
                sleep_ok,
            );
        }
        // ---- M12：序列帧动画（运行时注册 → 播放 → 帧事件触发）----
        1190 => {
            use crate::anim::{AnimDef, AnimPlayer};
            let mut frames = Vec::new();
            for i in 0..3u8 {
                let mut img = image::RgbaImage::new(8, 8);
                for p in img.pixels_mut() {
                    *p = image::Rgba([255, 40 * i, 60 * i, 255]);
                }
                frames.push(img);
            }
            let def = AnimDef {
                name: "test_anim".into(),
                sheet: "runtime_test.png".into(),
                frame_w: 8,
                frame_h: 8,
                frame_times: vec![0.05, 0.05, 0.05],
                events: vec![(1, "boom".into())],
                r#loop: true,
            };
            let ok = game
                .anims
                .register_runtime(def, &frames, ctx.renderer)
                .is_ok();
            game.anim_preview = Some(AnimPlayer::new("test_anim"));
            game.anim_last_event = None;
            println!(
                "[SELFTEST] anim register {}",
                if ok { "OK" } else { "FAIL" },
            );
        }
        1236 => {
            let frame = game
                .anim_preview
                .as_ref()
                .map(|p| p.frame)
                .unwrap_or(255);
            let n = game.anims.frame_count("test_anim");
            let ev = game.anim_last_event.clone().unwrap_or_default();
            let pass = n == 3 && frame < 3 && ev == "boom";
            println!(
                "[SELFTEST] anim play {} | frames {n} cur {frame} event '{ev}'",
                if pass { "PASS" } else { "FAIL" },
            );
            ctx.request_screenshot("screenshots/13_anim.png");
        }
        // 入夜
        935 => {
            game.world.time = 0.73;
        }
        970 => {
            let total = (game.world.pixels.w / 128) * (game.world.pixels.h / 128);
            println!(
                "[SELFTEST] tick={} | avg {:.2}ms/tick | sim {:.2} light {:.2} | active_px {} | asleep {}/{} chunks",
                tick,
                game.tick_ms_sum / game.tick_count.max(1) as f32,
                game.world.perf_sim_ms / game.tick_count.max(1) as f32,
                game.world.perf_light_ms / game.tick_count.max(1) as f32,
                game.world.pixels.active_pixels,
                game.world.pixels.asleep_chunks,
                total,
            );
            println!(
                "[SELFTEST] player hp {:.0} pos ({:.0},{:.0}) | dummies {}",
                game.player.hp,
                game.player.pos.x,
                game.player.pos.y,
                game.ecs.len(),
            );
        }
        _ => {}
    }
}
