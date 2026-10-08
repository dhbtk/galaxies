import type { Aster, AsterChartInfo, DisplayableAster, StarSign } from '#/lib/chart.ts'
import * as React from 'react'
import styled from 'styled-components'
import { useState } from 'react'

export const signColor = (sign: StarSign) => {
  switch (sign) {
    case 'Leo':
    case 'Sagittarius':
    case 'Aries':
      return 'var(--fire-color)';
    case 'Aquarius':
    case 'Gemini':
    case 'Libra':
      return 'var(--air-color)';
    case 'Taurus':
    case 'Virgo':
    case 'Capricorn':
      return 'var(--earth-color)';
    case 'Scorpio':
    case 'Pisces':
    case 'Cancer':
      return 'var(--water-color)';
  }
}

const Wrapper = styled.div<{sign: StarSign, image: string, open: boolean}>`
  display: flex;
  flex-direction: column;
  height: ${props => props.open ? '36rem' : '8rem'};
  transition: height 0.3s ease-in-out;
  margin: 0.5rem;
  padding: 0.5rem;
  border: 1px solid color-mix(in srgb, ${props => signColor(props.sign)}, rgba(0, 0, 0, 0) 80%);
  border-radius: 0.5rem;
  box-shadow: 0 0 0.5rem rgba(0, 0, 0, 0.2);
  background-image: linear-gradient(to bottom, rgba(0, 0, 0, 0.5) 2.5rem, rgba(0, 0, 0, 0.0) 8rem), url(${props => props.image});
  background-size: cover;
  background-position: center;
  position: relative;
  overflow: hidden;
  text-shadow: 0 0 0.15rem rgba(0, 0, 0, 0.9);
`

const AsterName = styled.h2`
  font-variant: small-caps;
  font-weight: 600;
  margin: 0;
  padding: 0;
  font-size: 1.125rem;
  flex-shrink: 0;
`

const Sign = styled.div<{sign: StarSign}>`
  color: ${props => signColor(props.sign)};
  font-variant: small-caps;
  flex-shrink: 0;
  font-size: 0.85rem;
`

const InfoBox = styled.div<{open: boolean}>`
  height: 100%;
  opacity: ${props => props.open ? 1 : 0};
  transition: opacity 0.3s ease-in-out;
  display: flex;
  flex-direction: column;
  font-variant: small-caps;
  font-size: 0.8rem;
  justify-content: flex-end;
`

export const AsterDisplay: React.FC<{ aster: DisplayableAster, info: AsterChartInfo }> = ({ aster, info }) => {
  const [open, setOpen] = useState(false)

  return (
    <Wrapper sign={info.sign} image={'/' + info.object.image.imageUrl} open={open} onClick={() => setOpen(!open)}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: '8rem' }}>
        <AsterName>{aster}</AsterName>
        <Sign sign={info.sign}>{info.sign} {info.degrees.toFixed(1)}º</Sign>
      </div>
      <InfoBox open={open}>
        <span>
          {info.object.id}
        </span>
        <span style={{fontSize: '0.7rem'}}>
          Right ascension: {info.object.rightAscensionDeg.toFixed(1)}º
          Declination: {info.object.declinationDeg.toFixed(1)}º
        </span>
      </InfoBox>
    </Wrapper>
  )
}
