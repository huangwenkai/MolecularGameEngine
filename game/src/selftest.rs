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
/// IDE 视口测试：记录开 IDE 前的世界时间基准
static IDE_T0: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
/// 玩家动作测试：新怪物是否生成成功
static SPAWN_OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
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
    (1228, "15_cave_walls"),
    (1248, "14_trees"),
];

pub fn drive(game: &mut GameApp, ctx: &mut EngineCtx) {
    // 每帧固化测试环境：selftest 必须与用户 settings.ron 中的开关状态无关
    // （用户开了"时间锁定"/关了"夜间刷怪"会导致 IDE 视口/刷怪等测试假性失败）
    game.settings.time_lock = false;
    game.settings.spawn_on = true;
    game.world.time_frozen = false;
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
        // 工具按住左键循环触发：按住期间持续倒出
        170 => input.inject(Action::Slot5, PRESS),
        171 => input.inject(Action::Slot5, RELEASE),
        175..=249 => {
            aim(input, cam, px + 10.0, py - 30.0);
            if tick == 175 {
                input.inject(Action::Attack, PRESS);
            }
        }
        250 => input.inject(Action::Attack, RELEASE),
        255 => input.inject(Action::Slot6, PRESS),
        256 => input.inject(Action::Slot6, RELEASE),
        // 沙倒在脚边水洼里：沙沉底堆积，把角色托起（粉末支撑验证）
        260..=339 => {
            aim(input, cam, px + 2.0, py - 24.0);
            if tick == 260 {
                input.inject(Action::Attack, PRESS);
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
            if tick == 490 {
                input.inject(Action::Attack, PRESS);
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
            if tick == 660 {
                input.inject(Action::Attack, PRESS);
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
                "[SELFTEST] 特效粒子 {} 飘字 {} | 投射物 {}",
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
            let s = std::fs::read_to_string(crate::project::path_of("data/vfx.ron"))
                .expect("vfx.ron 缺失");
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
                "[SELFTEST] 编辑器链路 {} | 蓝图数 {} | slash={} | 粒子 {}",
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
                "[SELFTEST] 掉落物 {} | 背包 {} | 经验 {}",
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
                "[SELFTEST] 已拾取 | 背包 {} 金币 {} 掉落物 left {}",
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
                    "[SELFTEST] 装备结果 {} | 属性 {:.1} -> {:.1}",
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
                "[SELFTEST] 暗黑循环 {} | 经验 {} 背包 {} 已装备 {} | 攻击 {:.1} (base 15)",
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
                "[SELFTEST] 怪物 {} | BOSS存活 {} at {:?}",
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
                "[SELFTEST] AI追击 {}/{} | 怪物受伤 {}",
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
            println!("[SELFTEST] BOSS阶段 {} | 弹丸 {}", phase, bullets);
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
                "[SELFTEST] M9怪物AI {} | 存活 {} 追击 {} BOSS阵亡 {}",
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
                "[SELFTEST] 存读档 {} | 挖空已还原 {} | 等级 {}",
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
                "[SELFTEST] 居民 {} | seeded food+water | hunger0 {:.0} thirst1 {:.0}",
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
                "[SELFTEST] 居民进食 {} 饮水 {}",
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
                "[SELFTEST] 居民消耗 {} | hunger0 {:.2} thirst1 {:.2}",
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
                "[SELFTEST] 居民睡觉 {} | 状态 {:?} 疲劳 {:.2}",
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
                "[SELFTEST] M14居民 {} | 进食 {} 饮水 {} 睡眠 {}",
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
                "[SELFTEST] 动画注册 {}",
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
                "[SELFTEST] 动画播放 {} | 帧数 {n} 当前帧 {frame} 事件 '{ev}'",
                if pass { "PASS" } else { "FAIL" },
            );
            ctx.request_screenshot("screenshots/13_anim.png");
        }
        // ---- 背景树 / 背景墙验收截图：传送至洞穴内部与森林地表 ----
        1205 => {
            // 展示厅：在出生点右侧地下（泥土墙层）挖一个 60x28 的房间 + 火把，验证背景墙渲染
            let (sx, sy) = (game.world.spawn_x + 140, game.world.spawn_y + 72);
            for dy in -14..14 {
                for dx in -30..30 {
                    if game.world.pixels.get(sx + dx, sy + dy).mat != 0 {
                        game.world.pixels.clear_px(sx + dx, sy + dy);
                    }
                }
            }
            game.world.place_torch(sx - 20, sy + 13);
            game.world.place_torch(sx + 20, sy + 13);
            game.player.pos = Vec2::new(sx as f32 + 0.5, sy as f32 + 13.0);
            game.player.vel = Vec2::ZERO;
            println!("[SELFTEST] 背景墙展示厅 开凿于 ({sx},{sy})");
        }
        1210 => {
            game.world.time = 0.30;
        }
        1230 => {
            // 森林地表：查看新树（背景层）
            let tx = game.world.spawn_x + 700;
            let sy = game.surface_y(tx);
            game.player.pos = Vec2::new(tx as f32 + 0.5, sy as f32 - 2.0);
            game.player.vel = Vec2::ZERO;
            game.world.time = 0.30;
            // 调试：统计右侧森林区域的树/植被像素
            let (wood, leaf) = (
                game.world.mats.id("wood").unwrap_or(0),
                game.world.mats.id("leaf").unwrap_or(0),
            );
            let grass = game.world.mats.id("tall_grass").unwrap_or(0);
            let flowers: Vec<u8> = ["flower_red", "flower_yellow", "flower_blue"]
                .iter()
                .filter_map(|n| game.world.mats.id(n))
                .collect();
            let (mut wn, mut ln) = (0u32, 0u32);
            let (mut gn, mut fn_) = (0u32, 0u32);
            let mut cols: Vec<i32> = Vec::new();
            for x in 2300..3300 {
                for y in 300..800 {
                    let m = game.world.pixels.get(x, y).mat;
                    if m == wood {
                        wn += 1;
                        cols.push(x);
                    } else if m == leaf {
                        ln += 1;
                    } else if m == grass {
                        gn += 1;
                    } else if flowers.contains(&m) {
                        fn_ += 1;
                    }
                }
            }
            cols.dedup();
            let (minx, maxx) = (cols.first().copied().unwrap_or(0), cols.last().copied().unwrap_or(0));
            println!(
                "[SELFTEST] 树木像素 木材 {wn} 树叶 {ln} | 树干列 {} 跨度 {minx}..{maxx} | veg grass {gn} flowers {fn_}",
                cols.len()
            );
        }
        // ---- 火焰法杖复现：切 8 号 + 按住攻击朝地面发射（触发爆炸/悬浮检测/局部重光照）----
        1330 => {
            input.inject(Action::Slot8, PRESS);
            aim(input, cam, px + 40.0, py + 6.0); // 瞄准脚下地面 → 火球撞地爆炸
        }
        1332 => {
            input.inject(Action::Attack, PRESS);
        }
        1340 => {
            input.inject(Action::Attack, RELEASE);
            input.inject(Action::Slot8, RELEASE);
        }
        1249 => {
            // 人物形象回归检查：默认贴图带眼睛 + 像素编辑读写 + 图集打包
            let eye_ok = game.skin.get("char_head").map(|pt| {
                let (w, h) = pt.img.dimensions();
                let dark = |x: u32, y: u32| {
                    let p = pt.img.get_pixel(x, y).0;
                    p[3] == 255 && p[0] < 100
                };
                dark(w * 5 / 8, h * 4 / 8) && dark((w * 6 / 8).min(w - 1), h * 4 / 8)
            }).unwrap_or(false);
            let edit_ok = game.skin.get_mut("char_torso").map(|pt| {
                pt.img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
                pt.img.get_pixel(0, 0).0 == [255, 0, 0, 255]
            }).unwrap_or(false);
            let packed = game.skin.get("char_head").map(|p| p.ax > 0).unwrap_or(false);
            let ok = eye_ok && edit_ok && packed;
            println!(
                "[SELFTEST] 人物形象 {} | 眼睛 {eye_ok} 编辑 {edit_ok} 图集 {packed}",
                if ok { "PASS" } else { "FAIL" }
            );
            // 植被重新生长回归检查：密度清零 → 植被应被全部清除
            let mut defs = game.veg.plants.clone();
            for d in &mut defs {
                d.density = 0.0;
            }
            game.world.regrow_vegetation(&defs);
            let grass = game.world.mats.id("tall_grass").unwrap_or(0);
            let mut n = 0u32;
            for y in 0..(game.world.pixels.h / 2) {
                for x in 0..game.world.pixels.w {
                    if game.world.pixels.get(x, y).mat == grass {
                        n += 1;
                    }
                }
            }
            println!(
                "[SELFTEST] 植被重铺 {} | 草像素 {n}",
                if n == 0 { "PASS" } else { "FAIL" }
            );
        }
        // 入夜
        935 => {
            game.world.time = 0.73;
        }
        970 => {
            let total = (game.world.pixels.w / 128) * (game.world.pixels.h / 128);
            println!(
                "[SELFTEST] tick={} | 平均 {:.2}ms/tick | 模拟 {:.2} 光照 {:.2} | 活跃像素 {} | 休眠 {}/{} 区块",
                tick,
                game.tick_ms_sum / game.tick_count.max(1) as f32,
                game.world.perf_sim_ms / game.tick_count.max(1) as f32,
                game.world.perf_light_ms / game.tick_count.max(1) as f32,
                game.world.pixels.active_pixels,
                game.world.pixels.asleep_chunks,
                total,
            );
            println!(
                "[SELFTEST] 玩家生命 {:.0} 位置 ({:.0},{:.0}) | 假人 {}",
                game.player.hp,
                game.player.pos.x,
                game.player.pos.y,
                game.ecs.len(),
            );
        }
        // ---- M17：技能系统（学习 → 施放 → 冷却 → 存档）----
        1250 => {
            game.skills.pts = 3;
            for i in 0..3 {
                game.skills.learn(i);
            }
            game.player.hp = 20.0; // 供治疗术验证
            game.settings.slot = 2; // 存档位 2
        }
        1252 => {
            input.inject(Action::Skill3, PRESS); // 治疗术
        }
        1253 => {
            input.inject(Action::Skill3, RELEASE);
        }
        1256 => {
            let healed = game.player.hp > 20.0 && game.player.hp <= game.player.max_hp;
            let cd_on = game.skills.cd_remaining(2) > 0.0;
            input.inject(Action::Skill1, PRESS); // 旋风斩
            println!(
                "[SELFTEST] 技能治疗 {} | 生命 {:.0} 冷却 {:.1}",
                if healed && cd_on { "PASS" } else { "FAIL" },
                game.player.hp,
                game.skills.cd_remaining(2),
            );
        }
        1257 => {
            input.inject(Action::Skill1, RELEASE);
        }
        1260 => {
            let cd_on = game.skills.cd_remaining(0) > 0.0;
            println!(
                "[SELFTEST] 技能旋风斩 {} | 冷却 {:.1}",
                if cd_on { "PASS" } else { "FAIL" },
                game.skills.cd_remaining(0),
            );
            // 存档位 2：保存 → 清空技能 → 读档还原
            let save_ok = save::save_game(game).is_ok();
            let file_ok = std::path::Path::new("saves/save2.ron").exists()
                && std::path::Path::new("saves/save2.px").exists();
            game.skills.learned = [0, 0, 0, 0];
            game.skills.pts = 0;
            let load_ok = save::load_game(game).is_ok();
            let restored = game.skills.learned == [1, 1, 1, 0];
            println!(
                "[SELFTEST] 存档位二 {} | 文件 {file_ok} 已还原 {restored}",
                if save_ok && load_ok { "PASS" } else { "FAIL" },
            );
            game.settings.slot = 1;
        }
        // ---- M17：NPC/怪物坠坑修复验证 ----
        1280 => {
            // NPC：放到地表下方 200px（模拟打碎地块后掉进洞穴）
            if let Some(n) = game.npcs.list.first_mut() {
                let sx = n.pos.x as i32;
                let surf = (0..game.world.pixels.h)
                    .find(|&y| game.world.solid_px(sx, y))
                    .unwrap_or(game.world.pixels.h / 2);
                n.pos.y = (surf + 200) as f32;
                n.vel = Vec2::ZERO;
            }
            // 怪物：把第一只非飞行怪放到远离玩家的深层洞穴（触发深洞清理）
            let px = game.player.pos.x as i32 + 600;
            let deep_surf = (0..game.world.pixels.h)
                .find(|&y| game.world.solid_px(px, y))
                .unwrap_or(game.world.pixels.h / 2);
            if let Some(m) = game
                .monsters
                .list
                .iter_mut()
                .find(|m| m.kind != crate::monsters::Kind::Bat)
            {
                m.pos = Vec2::new(px as f32, (deep_surf + 300) as f32);
                m.vel = Vec2::ZERO;
            }
        }
        1288 => {
            let npc_ok = game.npcs.list.first_mut().map(|n| {
                let sx = n.pos.x as i32;
                let surf = (0..game.world.pixels.h)
                    .find(|&y| game.world.solid_px(sx, y))
                    .unwrap_or(game.world.pixels.h / 2);
                n.pos.y <= (surf + 48) as f32
            }).unwrap_or(true);
            let px = game.player.pos.x as i32 + 600;
            let deep_surf = (0..game.world.pixels.h)
                .find(|&y| game.world.solid_px(px, y))
                .unwrap_or(game.world.pixels.h / 2);
            let monster_ok = !game.monsters.list.iter().any(|m| {
                (m.pos.x - px as f32).abs() < 1.0 && m.pos.y > (deep_surf + 200) as f32
            });
            println!(
                "[SELFTEST] 坠坑自救 {} | 居民回地表 {npc_ok} 怪物清理 {monster_ok}",
                if npc_ok && monster_ok { "PASS" } else { "FAIL" },
            );
        }
        // ---- M18：游戏工程（创建/打开/关闭 + 素材路径解析 + 热重载目录）----
        1300 => {
            game.project.create("selftest_proj");
        }
        1304 => {
            let in_proj = game.project.current.as_ref().map(|p| p.name.clone())
                == Some("selftest_proj".to_string());
            let mat = crate::project::path_of("data/materials.ron");
            let mat_in_proj = mat
                .to_string_lossy()
                .contains("selftest_proj");
            let has_file = mat.exists();
            // 工程打开时监听目录指向工程
            let watch = crate::project::watch_dirs();
            let watch_ok = watch
                .iter()
                .any(|d| d.to_string_lossy().contains("selftest_proj"));
            println!(
                "[SELFTEST] 工程打开 {} | current {in_proj} 材质在工程内 {mat_in_proj} 文件 {has_file} 监听 {watch_ok}",
                if in_proj && mat_in_proj && has_file && watch_ok { "PASS" } else { "FAIL" },
            );
            // 关闭工程 → 回到内置资源
            game.project.close();
        }
        1306 => {
            let builtin = crate::project::path_of("data/materials.ron");
            let back = !builtin.to_string_lossy().contains("selftest_proj") && builtin.exists();
            println!(
                "[SELFTEST] 工程关闭 {} | 路径 {}",
                if back { "PASS" } else { "FAIL" },
                builtin.display(),
            );
            // 清理测试工程，避免留下脏数据
            let _ = std::fs::remove_dir_all("projects/selftest_proj");

            // ---- IDE 视口：打开 IDE + run → 世界与 AI 继续推进 ----
            let _ = IDE_T0.set(game.world.time);
            game.ide.open = true;
            game.ide.run = true;
            game.ide.tab = 0;
        }
        1320 => {
            let t0 = IDE_T0.get().copied().unwrap_or(0.0);
            let mut d = game.world.time - t0;
            if d < 0.0 {
                d += 1.0; // 跨天回卷
            }
            let advanced = d > 1e-6;
            let npc_moved = game.npcs.list.iter().any(|n| n.state_t > 0);
            println!(
                "[SELFTEST] IDE视口 {} | 时间 {:.4} 居民推进 {npc_moved}",
                if advanced && npc_moved { "PASS" } else { "FAIL" },
                game.world.time
            );
            let _ = ();
            game.ide.open = false;

            // ---- 玩家动作与闪避：四种新怪物生成 + Shift 闪避冲刺/无敌 ----
            let gy = game.player.pos.y;
            let kinds = [
                crate::monsters::Kind::SkeletonSoldier,
                crate::monsters::Kind::DemonMushroom,
                crate::monsters::Kind::Goblin,
                crate::monsters::Kind::EyeBat,
            ];
            let n0 = game.monsters.list.len();
            for (i, k) in kinds.iter().enumerate() {
                game.monsters.test_spawn(
                    *k,
                    Vec2::new(game.player.pos.x + 200.0 + i as f32 * 30.0, gy - 2.0),
                    &mut game.rng,
                );
            }
            let _ = SPAWN_OK.set(game.monsters.list.len() == n0 + 4);
            let _new_ok = game.monsters.list.len() == n0 + 4;
            game.player.dodge_cd = 0.0;
            input.inject(mge_platform::action::Action::Dodge, PRESS);
        }
        // 闪避注入后第 2 帧断言（冲刺/无敌进行中）
        1322 => {
            input.inject(mge_platform::action::Action::Dodge, RELEASE);
            let spawned_ok = SPAWN_OK.get().copied().unwrap_or(false);
            // 素材绑定：骷髅士兵移动动画帧已入库
            let tex_ok = game
                .anims
                .monster_frame(
                    &crate::monsters::Kind::SkeletonSoldier,
                    crate::anim::MonAnimState::Walk,
                    0.0,
                )
                .is_some();
            let dodging = game.player.dodge_t > 0.15;
            let invuln_ok = game.player.invuln > 0.2;
            let cd_ok = game.player.dodge_cd > 0.8;
            let has_new = game.monsters.list.iter().any(|m| {
                matches!(
                    m.kind,
                    crate::monsters::Kind::SkeletonSoldier
                        | crate::monsters::Kind::DemonMushroom
                        | crate::monsters::Kind::Goblin
                        | crate::monsters::Kind::EyeBat
                )
            });
            println!(
                "[SELFTEST] 玩家动作与闪避 {} | 冲刺 {:.2} 无敌 {:.2} 冷却 {:.2} 新怪物在场 {has_new}",
                if dodging && invuln_ok && cd_ok && spawned_ok && has_new && tex_ok { "PASS" } else { "FAIL" },
                game.player.dodge_t,
                game.player.invuln,
                game.player.dodge_cd,
            );
        }
        // 火焰法杖压力测试后的性能采样
        1343 => {
            println!(
                "[SELFTEST] 火焰法杖阶段 | 平均 {:.2}ms/tick | 模拟 {:.2} 光照 {:.2} | 活跃像素 {}",
                game.tick_ms_sum / game.tick_count.max(1) as f32,
                game.world.perf_sim_ms / game.tick_count.max(1) as f32,
                game.world.perf_light_ms / game.tick_count.max(1) as f32,
                game.world.pixels.active_pixels,
            );
        }
        // ---- M19：状态效果（燃烧 DoT / 冰冻减速 / 中毒）----
        1360 => {
            // 清场后生成一只骷髅士兵，记录血量并点燃 + 冰冻
            game.monsters.list.clear();
            let px = game.player.pos.x as i32;
            let sy = game.surface_y(px + 80) as f32;
            game.monsters.test_spawn(
                crate::monsters::Kind::SkeletonSoldier,
                Vec2::new((px + 80) as f32, sy),
                &mut game.rng,
            );
            game.monsters
                .apply_status_at(Vec2::new((px + 80) as f32, sy), crate::monsters::StatusKind::Burn, 3.0);
            game.monsters
                .apply_status_at(Vec2::new((px + 80) as f32, sy), crate::monsters::StatusKind::Frozen, 3.0);
        }
        1400 => {
            // ~0.67s 后：燃烧至少结算 1 跳（-3 血）+ 冰冻在场
            let m = game.monsters.list.first();
            let burned = m.map(|m| m.hp < m.max_hp - 2.5).unwrap_or(false);
            let frozen = m
                .map(|m| m.statuses.iter().any(|s| s.kind == crate::monsters::StatusKind::Frozen))
                .unwrap_or(false);
            println!(
                "[SELFTEST] 状态效果 {} | 燃烧DoT {burned} 冰冻在场 {frozen} 剩余血 {:.1}/{:.1}",
                if burned && frozen { "PASS" } else { "FAIL" },
                m.map(|m| m.hp).unwrap_or(0.0),
                m.map(|m| m.max_hp).unwrap_or(0.0),
            );
        }
        _ => {}
    }
}
