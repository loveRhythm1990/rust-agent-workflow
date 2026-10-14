"""Render the README architecture diagram. Requires Pillow and a CJK font.
Usage: python scripts/render_workflow.py --font /path/to/CJK-font.ttf
"""
from pathlib import Path
import argparse
from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser()
parser.add_argument("--font", required=True)
args = parser.parse_args()
S = 2
im = Image.new("RGB", (1600*S, 850*S), "#f6f8fc")
d = ImageDraw.Draw(im)

def font(n): return ImageFont.truetype(args.font, n*S)
def text(x,y,t,n=22,c="#24334b"):
    d.text((x*S,y*S),t,font=font(n),fill=c)
def box(x,y,w,h,title,sub,color="#ffffff",edge="#dbe3ef"):
    d.rounded_rectangle((x*S,y*S,(x+w)*S,(y+h)*S),radius=16*S,fill=color,outline=edge,width=2*S)
    text(x+22,y+20,title,24)
    for i,line in enumerate(sub.split("\n")): text(x+22,y+60+i*30,line,19,"#596b83")
def arrow(points,c="#8a9bb3"):
    q=[(x*S,y*S) for x,y in points]
    d.line(q,fill=c,width=3*S)
    x,y=points[-1];px,py=points[-2]
    if x>px: tri=[(x,y),(x-10,y-6),(x-10,y+6)]
    elif x<px: tri=[(x,y),(x+10,y-6),(x+10,y+6)]
    elif y>py: tri=[(x,y),(x-6,y-10),(x+6,y-10)]
    else: tri=[(x,y),(x-6,y+10),(x+6,y+10)]
    d.polygon([(a*S,b*S) for a,b in tri],fill=c)

text(60,35,"RUST AGENT WORKFLOW",17,"#3267cc")
text(60,70,"可验证的编程任务协作",38)
text(60,128,"固定角色 · 同一候选快照 · 独立验收 · 有限修复",22,"#596b83")
box(60,185,330,115,"用户提交任务","目标 / 修改范围 / 验收标准")
arrow([(390,243),(430,243)])
box(430,185,1110,115,"Rust Workflow Controller","管理状态、执行权限、预算、会话与人工门禁", "#eaf0ff", "#a8bee9")
arrow([(560,300),(560,325),(180,325),(180,350)])
steps=[("01  编码 / 修复","Codex CLI\n独立任务工作区"),("02  冻结快照","停止写入\n绑定候选 SHA"),("03  独立验收","可信 Runner\n固定验收计划"),("04  独立审查","Claude Code CLI\n审查同一候选"),("05  核实发现","复现 / 反证\n必要时人工判断")]
for i,(title,sub) in enumerate(steps):
    x=60+i*300;box(x,350,280,140,title,sub)
    if i<4:arrow([(x+280,420),(x+300,420)])
arrow([(1400,490),(1400,555),(935,555)])
box(665,510,270,105,"确定性决策","依据有效证据与预算", "#eaf0ff", "#a8bee9")
arrow([(665,565),(180,565),(180,490)],"#3267cc")
text(280,530,"允许修复且预算足够：生成新候选",19,"#3267cc")
arrow([(745,615),(745,640),(480,640),(480,675)],"#338266")
arrow([(855,615),(855,640),(1120,640),(1120,675)],"#bd8531")
box(270,675,420,110,"通过 → 交付候选与证据","验收通过，无未决阻断；由用户集成", "#eaf6ef", "#abd7be")
box(910,675,470,110,"停止 / 等待人工 → 保存现场","失败、证据不足、超限或取消；保留结论", "#fff5e5", "#e4c895")
text(60,812,"运行结束 ≠ 验收通过  |  新候选使旧证据失效  |  无效报告进入异常决策",18,"#596b83")
out=Path(__file__).resolve().parents[1]/"docs/images/workflow.png"
out.parent.mkdir(parents=True,exist_ok=True)
im.save(out)
print(out)
